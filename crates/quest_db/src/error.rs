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

    /// Returned when a quest is not found by name and expansion source.
    #[error("Quest not found: name='{name}', source='{source_name}'")]
    QuestNotFoundByName { name: String, source_name: String },

    /// Returned when quest progress is not found for a playthrough and quest.
    #[error("Quest progress not found for playthrough {playthrough_id} and quest {quest_id}")]
    ProgressNotFound { playthrough_id: i64, quest_id: i64 },

    /// General invalid data error.
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, QuestTrackerError>;
