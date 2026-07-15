use nova_kernel::{ErrorCategory, NovaError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KnowledgeError {
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    #[error("entity already exists: {0}")]
    EntityAlreadyExists(String),
    #[error("relationship not found: {0}")]
    RelationshipNotFound(String),
    #[error("duplicate entity: {0}")]
    DuplicateEntity(String),
    #[error("timeline not available: {0}")]
    TimelineNotAvailable(String),
    #[error("summary generation failed: {0}")]
    SummaryFailed(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("recall failed: {0}")]
    RecallFailed(String),
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error("module not initialized")]
    NotInitialized,
    #[error("index error: {0}")]
    IndexError(String),
    #[error("reasoning error: {0}")]
    ReasoningError(String),
    #[error("embedding error: {0}")]
    EmbeddingError(String),
    #[error("serialization error: {0}")]
    SerializationError(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("entity extraction failed: {0}")]
    ExtractionFailed(String),
    #[error("no path found between {0} and {1}")]
    NoPathFound(String, String),
}

impl From<KnowledgeError> for NovaError {
    fn from(e: KnowledgeError) -> Self {
        let category = match &e {
            KnowledgeError::Storage(_) => ErrorCategory::Storage,
            KnowledgeError::InvalidQuery(_) => ErrorCategory::ConfigInvalid,
            KnowledgeError::PermissionDenied(_) => ErrorCategory::EgressDenied,
            KnowledgeError::EmbeddingError(_) => ErrorCategory::Internal,
            KnowledgeError::IndexError(_) => ErrorCategory::Internal,
            _ => ErrorCategory::Internal,
        };
        NovaError::new(category, "ERR_KNOWLEDGE", &e.to_string())
    }
}
