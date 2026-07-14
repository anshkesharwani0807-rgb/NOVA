use nova_kernel::{ErrorCategory, NovaError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisionErrorCategory {
    ImageDecode,
    Ocr,
    Embedding,
    Caption,
    Face,
    Cache,
    Provider,
    Unsupported,
    Screenshot,
    Preprocessor,
    Context,
    Metadata,
}

impl VisionErrorCategory {
    fn as_str(&self) -> &'static str {
        match self {
            VisionErrorCategory::ImageDecode => "ERR_VISION_IMAGE_DECODE",
            VisionErrorCategory::Ocr => "ERR_VISION_OCR",
            VisionErrorCategory::Embedding => "ERR_VISION_EMBEDDING",
            VisionErrorCategory::Caption => "ERR_VISION_CAPTION",
            VisionErrorCategory::Face => "ERR_VISION_FACE",
            VisionErrorCategory::Cache => "ERR_VISION_CACHE",
            VisionErrorCategory::Provider => "ERR_VISION_PROVIDER",
            VisionErrorCategory::Unsupported => "ERR_VISION_UNSUPPORTED",
            VisionErrorCategory::Screenshot => "ERR_VISION_SCREENSHOT",
            VisionErrorCategory::Preprocessor => "ERR_VISION_PREPROCESSOR",
            VisionErrorCategory::Context => "ERR_VISION_CONTEXT",
            VisionErrorCategory::Metadata => "ERR_VISION_METADATA",
        }
    }
}

pub fn vision_error(cat: VisionErrorCategory, msg: &str) -> NovaError {
    NovaError::new(ErrorCategory::Internal, cat.as_str(), msg)
}
