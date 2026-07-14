use async_trait::async_trait;
use nova_kernel::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub color_space: String,
}

#[async_trait]
pub trait ImageDecoder: Send + Sync {
    async fn decode_rgba(&self, bytes: &[u8]) -> Result<DecodedImage>;
    async fn decode_grayscale(&self, bytes: &[u8]) -> Result<DecodedImage>;
    async fn thumbnail(&self, bytes: &[u8], max_w: u32, max_h: u32) -> Result<DecodedImage>;
}

pub struct NativeImageDecoder;

impl NativeImageDecoder {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NativeImageDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeImageDecoder {
    pub fn decode_with_orientation(&self, bytes: &[u8], orientation: u16) -> Result<DecodedImage> {
        use image::imageops;
        let img = self.load(bytes)?;
        let rgba = img.to_rgba8();
        let mut out = image::DynamicImage::from(rgba);
        match orientation {
            2 => {
                out = imageops::flip_horizontal(&out).into();
            }
            3 => {
                out = imageops::rotate180(&out).into();
            }
            4 => {
                let flipped = imageops::flip_horizontal(&out);
                out = imageops::rotate180(&flipped).into();
            }
            5 => {
                let rotated = imageops::rotate90(&out);
                out = imageops::flip_horizontal(&rotated).into();
            }
            6 => {
                out = imageops::rotate90(&out).into();
            }
            7 => {
                let rotated = imageops::rotate270(&out);
                out = imageops::flip_horizontal(&rotated).into();
            }
            8 => {
                out = imageops::rotate270(&out).into();
            }
            _ => {}
        }
        let result = out.to_rgba8();
        let (rw, rh) = result.dimensions();
        Ok(DecodedImage {
            width: rw,
            height: rh,
            data: result.to_vec(),
            channels: 4,
            color_space: "sRGB".to_string(),
        })
    }

    fn load(&self, bytes: &[u8]) -> Result<image::DynamicImage> {
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| {
                crate::error::vision_error(
                    crate::error::VisionErrorCategory::ImageDecode,
                    &format!("Format detection failed: {e}"),
                )
            })?;
        reader.decode().map_err(|e| {
            crate::error::vision_error(
                crate::error::VisionErrorCategory::ImageDecode,
                &format!("Decode failed: {e}"),
            )
        })
    }
}

#[async_trait]
impl ImageDecoder for NativeImageDecoder {
    async fn decode_rgba(&self, bytes: &[u8]) -> Result<DecodedImage> {
        let img = self.load(bytes)?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Ok(DecodedImage {
            width: w,
            height: h,
            data: rgba.to_vec(),
            channels: 4,
            color_space: "sRGB".to_string(),
        })
    }

    async fn decode_grayscale(&self, bytes: &[u8]) -> Result<DecodedImage> {
        let img = self.load(bytes)?;
        let gray = img.to_luma8();
        let (w, h) = gray.dimensions();
        Ok(DecodedImage {
            width: w,
            height: h,
            data: gray.to_vec(),
            channels: 1,
            color_space: "Gray".to_string(),
        })
    }

    async fn thumbnail(&self, bytes: &[u8], max_w: u32, max_h: u32) -> Result<DecodedImage> {
        let img = self.load(bytes)?;
        let thumb = img.thumbnail(max_w, max_h);
        let data = thumb.to_rgba8();
        let (w, h) = data.dimensions();
        Ok(DecodedImage {
            width: w,
            height: h,
            data: data.to_vec(),
            channels: 4,
            color_space: "sRGB".to_string(),
        })
    }
}
