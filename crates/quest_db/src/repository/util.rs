//! Row-mapping helpers shared by the repository modules.

use chrono::{DateTime, Utc};

use crate::models::Quest;

/// Quest columns, in the order expected by [`map_quest`]. Title and description come from the
/// `:lang` text row, falling back to English and then to the internal name.
pub(crate) const QUEST_COLUMNS: &str = "q.id, q.journal_path, q.base_name, q.source, q.quest_type,
    q.region, q.recommended_level, q.sort_order, q.wiki_page_id, q.wiki_title, q.important_notes,
    q.is_unmarked, q.cutoff_quest_id,
    COALESCE(t.title, te.title, q.base_name), COALESCE(t.description, te.description)";

/// Number of columns in [`QUEST_COLUMNS`]; columns selected after them start at this index.
pub(crate) const QUEST_COLUMN_COUNT: usize = 15;

/// `FROM` clause for [`QUEST_COLUMNS`]; binds the `:lang` named parameter.
pub(crate) const QUEST_FROM: &str = "FROM quests q
    LEFT JOIN quest_texts t  ON t.quest_id = q.id AND t.language = :lang
    LEFT JOIN quest_texts te ON te.quest_id = q.id AND te.language = 'en'";

/// Maps the leading [`QUEST_COLUMNS`] of a row. `prerequisite_ids` is left empty for the caller.
pub(crate) fn map_quest(row: &rusqlite::Row) -> rusqlite::Result<Quest> {
    Ok(Quest {
        id: row.get(0)?,
        journal_path: row.get(1)?,
        base_name: row.get(2)?,
        source: row.get(3)?,
        quest_type: row.get(4)?,
        region: row.get(5)?,
        recommended_level: row.get(6)?,
        sort_order: row.get(7)?,
        wiki_page_id: row.get(8)?,
        wiki_title: row.get(9)?,
        important_notes: row.get(10)?,
        is_unmarked: row.get(11)?,
        cutoff_quest_id: row.get(12)?,
        prerequisite_ids: Vec::new(),
        title: row.get(13)?,
        description: row.get(14)?,
    })
}

/// Parses timestamps written either by chrono (RFC 3339) or by SQLite's
/// `strftime('%Y-%m-%dT%H:%M:%SZ')` column defaults.
pub(crate) fn parse_timestamp(s: &str, field_name: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")
                .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
        })
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse timestamp for {field_name}: '{s}': {e}"),
                )),
            )
        })
}

/// Parses an optional timestamp column.
pub(crate) fn parse_optional_timestamp(
    s: Option<String>,
    field_name: &str,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    s.as_deref().map(|s| parse_timestamp(s, field_name)).transpose()
}
