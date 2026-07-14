use async_trait::async_trait;
use image::GenericImageView;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThumbnailMode {
    Fit,
    Fill,
    Crop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailResult {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub format: String,
}

#[async_trait]
pub trait ThumbnailGenerator: Send + Sync {
    async fn generate(
        &self,
        bytes: &[u8],
        max_w: u32,
        max_h: u32,
        mode: ThumbnailMode,
    ) -> Result<ThumbnailResult>;
}

pub struct NativeThumbnailGenerator {
    quality: u8,
}

impl NativeThumbnailGenerator {
    pub fn new(quality: u8) -> Self {
        Self { quality }
    }
}

#[async_trait]
impl ThumbnailGenerator for NativeThumbnailGenerator {
    async fn generate(
        &self,
        bytes: &[u8],
        max_w: u32,
        max_h: u32,
        mode: ThumbnailMode,
    ) -> Result<ThumbnailResult> {
        let img = image::load_from_memory(bytes).map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to load image for thumbnail: {e}"),
            )
        })?;

        let (w, h) = img.dimensions();
        let thumb = match mode {
            ThumbnailMode::Fit => {
                let scale = f64::min(max_w as f64 / w as f64, max_h as f64 / h as f64);
                if scale >= 1.0 {
                    img.clone()
                } else {
                    let nw = (w as f64 * scale) as u32;
                    let nh = (h as f64 * scale) as u32;
                    img.resize_exact(nw.max(1), nh.max(1), image::imageops::FilterType::Lanczos3)
                }
            }
            ThumbnailMode::Fill => {
                let scale = f64::max(max_w as f64 / w as f64, max_h as f64 / h as f64);
                let nw = (w as f64 * scale) as u32;
                let nh = (h as f64 * scale) as u32;
                img.resize_exact(nw.max(1), nh.max(1), image::imageops::FilterType::Lanczos3)
            }
            ThumbnailMode::Crop => img.thumbnail(max_w, max_h),
        };

        let rgba = thumb.to_rgba8();
        let rgb = image::DynamicImage::from(rgba).to_rgb8();
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, self.quality);
        encoder
            .encode(
                &rgb,
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| {
                crate::error::vision_error(
                    crate::error::VisionErrorCategory::ImageDecode,
                    &format!("Failed to encode thumbnail: {e}"),
                )
            })?;
        let data = buf.into_inner();

        Ok(ThumbnailResult {
            width: rgb.width(),
            height: rgb.height(),
            size_bytes: data.len() as u64,
            data,
            format: "jpeg".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image_bytes() -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let img = image::DynamicImage::new_rgba8(64, 64);
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[tokio::test]
    async fn test_thumbnail_fit() {
        let gen = NativeThumbnailGenerator::new(80);
        let bytes = test_image_bytes();
        let result = gen
            .generate(&bytes, 32, 32, ThumbnailMode::Fit)
            .await
            .unwrap();
        assert!(result.width <= 32);
        assert!(result.height <= 32);
        assert!(result.size_bytes > 0);
    }

    #[tokio::test]
    async fn test_thumbnail_crop() {
        let gen = NativeThumbnailGenerator::new(80);
        let bytes = test_image_bytes();
        let result = gen
            .generate(&bytes, 16, 16, ThumbnailMode::Crop)
            .await
            .unwrap();
        assert_eq!(result.width, 16);
        assert_eq!(result.height, 16);
    }
}
