use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    Storage,
    Inference,
    EgressDenied,
    ConsentRequired,
    ConfigInvalid,
    Plugin,
    Kernel,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[error("[{category:?}] Code {code}: {message} (Correlation: {correlation_id:?})")]
pub struct NovaError {
    pub category: ErrorCategory,
    pub code: String,
    pub message: String,
    pub correlation_id: Option<Uuid>,
}

impl NovaError {
    pub fn new(category: ErrorCategory, code: &str, message: &str) -> Self {
        Self {
            category,
            code: code.to_string(),
            message: message.to_string(),
            correlation_id: None,
        }
    }

    pub fn with_correlation(mut self, id: Uuid) -> Self {
        self.correlation_id = Some(id);
        self
    }
}

pub type Result<T> = std::result::Result<T, NovaError>;
