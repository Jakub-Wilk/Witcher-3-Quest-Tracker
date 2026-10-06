use thiserror::Error;

/// The top-level error type for all quest_db operations.
#[derive(Debug, Error)]
pub enum QuestTrackerError {
    /// Wraps a raw rusqlite database error.
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    /// Wraps a rusqlite migration error.
    #[error("Migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    /// Returned when a requested Playthrough is not found.
    #[error("Playthrough not found: id={0}")]
    PlaythroughNotFound(i64),

    /// Returned when a requested Quest is not found.
    #[error("Quest not found: id={0}")]
    QuestNotFound(i64),

    /// General invalid data error.
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, QuestTrackerError>;
