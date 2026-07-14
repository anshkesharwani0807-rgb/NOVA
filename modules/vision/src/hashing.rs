use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageHash(pub u64);

pub const HAMMING_IDENTICAL: u32 = 0;
pub const HAMMING_SIMILAR: u32 = 10;
pub const HAMMING_DIFFERENT: u32 = 25;

impl ImageHash {
    pub fn hamming_distance(&self, other: &ImageHash) -> u32 {
        (self.0 ^ other.0).count_ones()
    }

    pub fn similarity(&self, other: &ImageHash) -> f64 {
        1.0 - (self.hamming_distance(other) as f64 / 64.0)
    }

    pub fn is_identical(&self, other: &ImageHash) -> bool {
        self.hamming_distance(other) == HAMMING_IDENTICAL
    }

    pub fn is_similar(&self, other: &ImageHash) -> bool {
        self.hamming_distance(other) <= HAMMING_SIMILAR
    }
}

#[async_trait]
pub trait ImageHasher: Send + Sync {
    async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash>;
    fn algorithm_name(&self) -> &'static str;
}

pub struct AverageHasher;

impl AverageHasher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AverageHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl AverageHasher {
    fn compute(img: &image::DynamicImage) -> ImageHash {
        let small = img.resize_exact(8, 8, image::imageops::FilterType::Lanczos3);
        let gray = small.to_luma8();
        let pixels: Vec<u8> = gray.to_vec();
        let avg: u32 = pixels.iter().map(|&p| p as u32).sum::<u32>() / pixels.len() as u32;
        let mut hash: u64 = 0;
        for (i, &p) in pixels.iter().enumerate() {
            if p as u32 > avg {
                hash |= 1 << (63 - i);
            }
        }
        ImageHash(hash)
    }
}

#[async_trait]
impl ImageHasher for AverageHasher {
    async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash> {
        let img = image::load_from_memory(bytes).map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to load for average hash: {e}"),
            )
        })?;
        Ok(Self::compute(&img))
    }

    fn algorithm_name(&self) -> &'static str {
        "average"
    }
}

pub struct DifferenceHasher;

impl DifferenceHasher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DifferenceHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl DifferenceHasher {
    fn compute(img: &image::DynamicImage) -> ImageHash {
        let small = img.resize_exact(9, 8, image::imageops::FilterType::Lanczos3);
        let gray = small.to_luma8();
        let mut hash: u64 = 0;
        for y in 0..8 {
            for x in 0..8 {
                let left = gray.get_pixel(x, y)[0];
                let right = gray.get_pixel(x + 1, y)[0];
                if left > right {
                    hash |= 1 << (63 - (y * 8 + x));
                }
            }
        }
        ImageHash(hash)
    }
}

#[async_trait]
impl ImageHasher for DifferenceHasher {
    async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash> {
        let img = image::load_from_memory(bytes).map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to load for difference hash: {e}"),
            )
        })?;
        Ok(Self::compute(&img))
    }

    fn algorithm_name(&self) -> &'static str {
        "difference"
    }
}

pub struct PerceptualHasher;

impl PerceptualHasher {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PerceptualHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl PerceptualHasher {
    fn compute(img: &image::DynamicImage) -> ImageHash {
        let small = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
        let gray = small.to_luma8();
        let dct = Self::dct_8x8(&gray);
        let mut hash: u64 = 0;
        let median = Self::median(&dct);
        for (i, &val) in dct.iter().enumerate() {
            if val > median {
                hash |= 1 << (63 - i);
            }
        }
        ImageHash(hash)
    }

    fn dct_8x8(gray: &image::ImageBuffer<image::Luma<u8>, Vec<u8>>) -> [f64; 64] {
        let mut block = [0.0f64; 64];
        for y in 0..8 {
            for x in 0..8 {
                block[y * 8 + x] = gray.get_pixel(x as u32, y as u32)[0] as f64;
            }
        }
        let mut out = [0.0f64; 64];
        for v in 0..8 {
            for u in 0..8 {
                let mut sum = 0.0;
                for y in 0..8 {
                    for x in 0..8 {
                        let px = block[y * 8 + x];
                        let cu = if u == 0 { 1.0 / f64::sqrt(2.0) } else { 1.0 };
                        let cv = if v == 0 { 1.0 / f64::sqrt(2.0) } else { 1.0 };
                        sum += cu
                            * cv
                            * px
                            * f64::cos(
                                ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI) / 16.0,
                            )
                            * f64::cos(
                                ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI) / 16.0,
                            );
                    }
                }
                out[v * 8 + u] = 0.25 * sum;
            }
        }
        out
    }

    fn median(arr: &[f64; 64]) -> f64 {
        let mut sorted = *arr;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[32]
    }
}

#[async_trait]
impl ImageHasher for PerceptualHasher {
    async fn hash_image(&self, bytes: &[u8]) -> Result<ImageHash> {
        let img = image::load_from_memory(bytes).map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to load for perceptual hash: {e}"),
            )
        })?;
        Ok(Self::compute(&img))
    }

    fn algorithm_name(&self) -> &'static str {
        "perceptual"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image_bytes() -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let img = image::DynamicImage::new_rgba8(16, 16);
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[tokio::test]
    async fn test_average_hash() {
        let hasher = AverageHasher::new();
        let bytes = test_image_bytes();
        let hash = hasher.hash_image(&bytes).await.unwrap();
        assert!(hash.is_identical(&hash));
    }

    #[tokio::test]
    async fn test_difference_hash() {
        let hasher = DifferenceHasher::new();
        let bytes = test_image_bytes();
        let hash = hasher.hash_image(&bytes).await.unwrap();
        assert!(hash.is_identical(&hash));
    }

    #[tokio::test]
    async fn test_perceptual_hash() {
        let hasher = PerceptualHasher::new();
        let bytes = test_image_bytes();
        let hash = hasher.hash_image(&bytes).await.unwrap();
        assert!(hash.is_identical(&hash));
    }

    #[test]
    fn test_hamming_distance() {
        let a = ImageHash(0b0000);
        let b = ImageHash(0b1111);
        assert_eq!(a.hamming_distance(&b), 4);
    }

    #[test]
    fn test_similarity() {
        let a = ImageHash(0);
        let b = ImageHash(u64::MAX);
        assert!((a.similarity(&b) - 0.0).abs() < 0.01);
    }
}
