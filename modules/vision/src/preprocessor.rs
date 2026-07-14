use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResizeMode {
    Fit,
    Fill,
    Crop,
    Pad,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Normalization {
    None,
    ZeroToOne,
    MinusOneToOne,
    Imagenet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub original_width: u32,
    pub original_height: u32,
    pub scale_x: f64,
    pub scale_y: f64,
}

pub struct ImagePreprocessor;

impl ImagePreprocessor {
    pub fn new() -> Self {
        Self
    }

    pub fn resize(
        &self,
        bytes: &[u8],
        target_w: u32,
        target_h: u32,
        mode: ResizeMode,
    ) -> Result<PreprocessedImage> {
        let img = self.load(bytes)?;
        let (orig_w, orig_h) = (img.width(), img.height());

        let (new_w, new_h) = match mode {
            ResizeMode::Exact => (target_w, target_h),
            ResizeMode::Fit => self.fit_dimensions(orig_w, orig_h, target_w, target_h),
            ResizeMode::Fill => self.fill_dimensions(orig_w, orig_h, target_w, target_h),
            ResizeMode::Crop => {
                let scale = (target_w as f64 / orig_w as f64).max(target_h as f64 / orig_h as f64);
                let scaled_w = (orig_w as f64 * scale) as u32;
                let scaled_h = (orig_h as f64 * scale) as u32;
                let x = (scaled_w.saturating_sub(target_w)) / 2;
                let y = (scaled_h.saturating_sub(target_h)) / 2;
                let mut resized =
                    img.resize_exact(scaled_w, scaled_h, image::imageops::FilterType::Lanczos3);
                let cropped =
                    image::imageops::crop(&mut resized, x, y, target_w, target_h).to_image();
                return self.to_result(cropped.into(), orig_w, orig_h);
            }
            ResizeMode::Pad => {
                let (fw, fh) = self.fit_dimensions(orig_w, orig_h, target_w, target_h);
                let resized = img.resize_exact(fw, fh, image::imageops::FilterType::Lanczos3);
                let mut canvas = image::RgbaImage::new(target_w, target_h);
                let ox = (target_w.saturating_sub(fw)) / 2;
                let oy = (target_h.saturating_sub(fh)) / 2;
                image::imageops::overlay(&mut canvas, &resized.to_rgba8(), ox as i64, oy as i64);
                return self.to_result(canvas.into(), orig_w, orig_h);
            }
        };

        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        self.to_result(resized, orig_w, orig_h)
    }

    pub fn normalize_to_range(bytes: &[u8], normalization: Normalization) -> Vec<f32> {
        let pixels: Vec<f32> = bytes.iter().map(|&b| b as f32).collect();
        match normalization {
            Normalization::None => pixels,
            Normalization::ZeroToOne => pixels.iter().map(|&v| v / 255.0).collect(),
            Normalization::MinusOneToOne => {
                pixels.iter().map(|&v| (v / 255.0) * 2.0 - 1.0).collect()
            }
            Normalization::Imagenet => pixels
                .iter()
                .map(|&v| (v / 255.0 - 0.485) / 0.229)
                .collect(),
        }
    }

    pub fn to_rgba(bytes: &[u8]) -> Result<Vec<u8>> {
        let img = Self::load_static(bytes)?;
        Ok(img.to_rgba8().to_vec())
    }

    pub fn to_grayscale(bytes: &[u8]) -> Result<Vec<u8>> {
        let img = Self::load_static(bytes)?;
        Ok(img.to_luma8().to_vec())
    }

    pub fn ensure_min_size(bytes: &[u8], min_w: u32, min_h: u32) -> Result<PreprocessedImage> {
        let img = Self::load_static(bytes)?;
        let (w, h) = (img.width(), img.height());
        if w >= min_w && h >= min_h {
            return Self::to_result_static(img, w, h);
        }
        let scale = (min_w as f64 / w as f64).max(min_h as f64 / h as f64);
        let new_w = (w as f64 * scale) as u32;
        let new_h = (h as f64 * scale) as u32;
        let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
        Self::to_result_static(resized, w, h)
    }

    fn load(&self, bytes: &[u8]) -> Result<image::DynamicImage> {
        Self::load_static(bytes)
    }

    fn load_static(bytes: &[u8]) -> Result<image::DynamicImage> {
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| {
                nova_kernel::NovaError::new(
                    nova_kernel::ErrorCategory::Internal,
                    "ERR_VISION_PREPROCESSOR_FORMAT",
                    &format!("Failed to detect image format: {e}"),
                )
            })?;
        reader.decode().map_err(|e| {
            nova_kernel::NovaError::new(
                nova_kernel::ErrorCategory::Internal,
                "ERR_VISION_PREPROCESSOR_DECODE",
                &format!("Failed to decode image: {e}"),
            )
        })
    }

    fn fit_dimensions(&self, w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
        Self::fit_dimensions_static(w, h, max_w, max_h)
    }

    fn fit_dimensions_static(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
        let ratio = (max_w as f64 / w as f64).min(max_h as f64 / h as f64);
        (
            (w as f64 * ratio).round() as u32,
            (h as f64 * ratio).round() as u32,
        )
    }

    fn fill_dimensions(&self, w: u32, h: u32, min_w: u32, min_h: u32) -> (u32, u32) {
        let ratio = (min_w as f64 / w as f64).max(min_h as f64 / h as f64);
        (
            (w as f64 * ratio).round() as u32,
            (h as f64 * ratio).round() as u32,
        )
    }

    fn to_result(
        &self,
        img: image::DynamicImage,
        orig_w: u32,
        orig_h: u32,
    ) -> Result<PreprocessedImage> {
        Self::to_result_static(img, orig_w, orig_h)
    }

    fn to_result_static(
        img: image::DynamicImage,
        orig_w: u32,
        orig_h: u32,
    ) -> Result<PreprocessedImage> {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(PreprocessedImage {
            data: rgba.to_vec(),
            width: w,
            height: h,
            channels: 4,
            original_width: orig_w,
            original_height: orig_h,
            scale_x: w as f64 / orig_w as f64,
            scale_y: h as f64 / orig_h as f64,
        })
    }
}

impl Default for ImagePreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn make_test_image(w: u32, h: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        let img = image::RgbaImage::new(w, h);
        encoder
            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .expect("PNG encode");
        buf
    }

    #[test]
    fn test_resize_exact() {
        let p = ImagePreprocessor::new();
        let bytes = make_test_image(200, 100);
        let result = p.resize(&bytes, 100, 50, ResizeMode::Exact).unwrap();
        assert_eq!(result.width, 100);
        assert_eq!(result.height, 50);
    }

    #[test]
    fn test_resize_fit() {
        let p = ImagePreprocessor::new();
        let bytes = make_test_image(200, 100);
        let result = p.resize(&bytes, 100, 100, ResizeMode::Fit).unwrap();
        assert_eq!(result.width, 100);
        assert_eq!(result.height, 50);
    }

    #[test]
    fn test_resize_fill() {
        let p = ImagePreprocessor::new();
        let bytes = make_test_image(200, 100);
        let result = p.resize(&bytes, 50, 50, ResizeMode::Fill).unwrap();
        assert_eq!(result.width, 100);
        assert_eq!(result.height, 50);
    }

    #[test]
    fn test_normalize_zero_to_one() {
        let bytes = vec![0u8, 128, 255];
        let normalized = ImagePreprocessor::normalize_to_range(&bytes, Normalization::ZeroToOne);
        assert!((normalized[0] - 0.0).abs() < 0.01);
        assert!((normalized[1] - 128.0 / 255.0).abs() < 0.01);
        assert!((normalized[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize_minus_one_to_one() {
        let bytes = vec![0u8, 128, 255];
        let normalized =
            ImagePreprocessor::normalize_to_range(&bytes, Normalization::MinusOneToOne);
        assert!((normalized[0] - (-1.0)).abs() < 0.01);
        assert!((normalized[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ensure_min_size() {
        let bytes = make_test_image(50, 30);
        let result = ImagePreprocessor::ensure_min_size(&bytes, 100, 100).unwrap();
        assert!(result.width >= 100);
        assert!(result.height >= 100);
    }

    #[test]
    fn test_to_rgba() {
        let bytes = make_test_image(10, 10);
        let rgba = ImagePreprocessor::to_rgba(&bytes).unwrap();
        assert_eq!(rgba.len(), 10 * 10 * 4);
    }

    #[test]
    fn test_resize_crop() {
        let p = ImagePreprocessor::new();
        let bytes = make_test_image(200, 100);
        let result = p.resize(&bytes, 50, 50, ResizeMode::Crop).unwrap();
        assert_eq!(result.width, 50);
        assert_eq!(result.height, 50);
    }

    #[test]
    fn test_resize_pad() {
        let p = ImagePreprocessor::new();
        let bytes = make_test_image(100, 200);
        let result = p.resize(&bytes, 200, 200, ResizeMode::Pad).unwrap();
        assert_eq!(result.width, 200);
        assert_eq!(result.height, 200);
    }
}
