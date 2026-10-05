use thiserror::Error;

/// Error types for quest scraping and parsing operations.
#[derive(Debug, Error)]
pub enum ScraperError {
    /// Wraps reqwest HTTP network or client errors.
    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    /// Wraps serde_json deserialization errors.
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    /// The MediaWiki API returned an error or an unexpected response.
    #[error("Wiki API error: {0}")]
    ApiError(String),

    /// The page has no quest infobox (e.g. an overview page or a minigame).
    #[error("Not a quest page: {0}")]
    NotAQuest(String),

    /// The quest infobox could not be interpreted.
    #[error("Failed to parse '{page}': {reason}")]
    ParseError { page: String, reason: String },
}

pub type Result<T> = std::result::Result<T, ScraperError>;
