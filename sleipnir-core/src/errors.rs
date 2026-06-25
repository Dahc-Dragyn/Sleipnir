use thiserror::Error;

#[derive(Error, Debug)]
pub enum SleipnirError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid payload format")]
    InvalidPayload,
}
