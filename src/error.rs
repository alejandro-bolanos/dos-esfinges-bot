use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum BotError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Zulip API error: {0}")]
    ZulipApi(String),
}
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, BotError>;
