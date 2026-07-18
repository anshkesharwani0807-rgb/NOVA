use nova_kernel::NovaError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Unsupported action: {0}")]
    UnsupportedAction(String),

    #[error("Coordinate out of bounds: x={x}, y={y}, screen=({w}x{h})")]
    OutOfBounds { x: i32, y: i32, w: u32, h: u32 },
}

pub type InputResult<T> = Result<T, InputError>;

impl From<InputError> for NovaError {
    fn from(e: InputError) -> Self {
        NovaError::new(
            nova_kernel::ErrorCategory::Internal,
            "ERR_INPUT",
            &e.to_string(),
        )
    }
}
