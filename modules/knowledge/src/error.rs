use nova_kernel::{ErrorCategory, NovaError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KnowledgeError {
    #[error("entity not found: {0}")]
    EntityNotFound(String),
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
}

impl From<KnowledgeError> for NovaError {
    fn from(e: KnowledgeError) -> Self {
        let category = match &e {
            KnowledgeError::Storage(_) => ErrorCategory::Storage,
            KnowledgeError::InvalidQuery(_) => ErrorCategory::ConfigInvalid,
            _ => ErrorCategory::Internal,
        };
        NovaError::new(category, "ERR_KNOWLEDGE", &e.to_string())
    }
}
