use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub mime_type: String,
}

#[async_trait]
pub trait ImageLoader: Send + Sync {
    async fn load_from_path(&self, path: &str) -> Result<LoadedImage>;
    async fn load_from_bytes(&self, bytes: &[u8]) -> Result<LoadedImage>;
    fn supported_formats(&self) -> Vec<&'static str>;
}

pub struct NativeImageLoader;

impl NativeImageLoader {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeImageLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeImageLoader {
    fn decode(img: &image::DynamicImage) -> LoadedImage {
        let format = match img {
            image::DynamicImage::ImageLuma8(_) => "grayscale",
            image::DynamicImage::ImageLumaA8(_) => "grayscale_alpha",
            image::DynamicImage::ImageRgb8(_) => "rgb",
            image::DynamicImage::ImageRgba8(_) => "rgba",
            _ => "unknown",
        };
        let mime = match format {
            "grayscale" | "grayscale_alpha" => "image/x-gray",
            "rgb" => "image/x-rgb",
            "rgba" => "image/x-rgba",
            _ => "application/octet-stream",
        };
        let data = img.clone().into_bytes();
        LoadedImage {
            width: img.width(),
            height: img.height(),
            data,
            format: format.to_string(),
            mime_type: mime.to_string(),
        }
    }
}

#[async_trait]
impl ImageLoader for NativeImageLoader {
    async fn load_from_path(&self, path: &str) -> Result<LoadedImage> {
        let img = image::open(path).map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to open image at '{path}': {e}"),
            )
        })?;
        Ok(Self::decode(&img))
    }

    async fn load_from_bytes(&self, bytes: &[u8]) -> Result<LoadedImage> {
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| {
                crate::error::vision_error(
                    crate::error::VisionErrorCategory::ImageDecode,
                    &format!("Failed to guess image format: {e}"),
                )
            })?;
        let img = reader.decode().map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to decode image: {e}"),
            )
        })?;
        Ok(Self::decode(&img))
    }

    fn supported_formats(&self) -> Vec<&'static str> {
        vec!["jpeg", "png", "webp", "bmp", "gif", "tiff"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_from_bytes_invalid() {
        let loader = NativeImageLoader::new();
        let result = loader.load_from_bytes(b"not an image").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_supported_formats() {
        let loader = NativeImageLoader::new();
        let formats = loader.supported_formats();
        assert!(formats.contains(&"jpeg"));
        assert!(formats.contains(&"png"));
    }
}
