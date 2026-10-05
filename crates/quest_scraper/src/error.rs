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

    /// Returned when a requested wiki page or quest is not found (e.g. HTTP 404).
    #[error("Page not found: {0}")]
    PageNotFound(String),

    /// Returned when infobox HTML parsing fails for a specific field.
    #[error("Failed to parse infobox field '{field_name}': {reason}")]
    ParseError { field_name: String, reason: String },

    /// Returned when MediaWiki API returns an error response.
    #[error("Wiki API error: {0}")]
    ApiError(String),
}

pub type Result<T> = std::result::Result<T, ScraperError>;
