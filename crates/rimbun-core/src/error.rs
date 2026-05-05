use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("invalid input: {0}")]
    InvalidInput(String),
}
