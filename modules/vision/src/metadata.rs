use async_trait::async_trait;
use image::GenericImageView;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub color_space: String,
    pub file_size: u64,
    pub orientation: u16,
    pub has_exif: bool,
    pub exif_fields: Vec<(String, String)>,
    pub has_gps: bool,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub bits_per_pixel: u8,
    pub is_animated: bool,
}

#[async_trait]
pub trait MetadataReader: Send + Sync {
    async fn read_metadata(&self, bytes: &[u8]) -> Result<ImageMetadata>;
}

pub struct NativeMetadataReader;

impl NativeMetadataReader {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeMetadataReader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetadataReader for NativeMetadataReader {
    async fn read_metadata(&self, bytes: &[u8]) -> Result<ImageMetadata> {
        let img = image::load_from_memory(bytes).map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Failed to load image for metadata: {e}"),
            )
        })?;
        let (w, h) = img.dimensions();
        Ok(ImageMetadata {
            width: w,
            height: h,
            format: "unknown".to_string(),
            color_space: "sRGB".to_string(),
            file_size: bytes.len() as u64,
            orientation: 1,
            has_exif: false,
            exif_fields: vec![],
            has_gps: false,
            gps_latitude: None,
            gps_longitude: None,
            bits_per_pixel: 24,
            is_animated: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_metadata_invalid() {
        let reader = NativeMetadataReader::new();
        let result = reader.read_metadata(b"bad data").await;
        assert!(result.is_err());
    }
}
