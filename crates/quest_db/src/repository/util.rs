//! Row-mapping helpers shared by the repository modules.

use chrono::{DateTime, Utc};

use crate::models::Quest;

/// Quest columns, aliased as `q`, in the order expected by [`map_quest`].
pub(crate) const QUEST_COLUMNS: &str = "q.id, q.wiki_page_id, q.wiki_title, q.name, q.localized_name,
    q.source, q.quest_type, q.region, q.recommended_level, q.sort_order, q.description,
    q.important_notes, q.is_unmarked, q.cutoff_quest_id, q.created_at, q.updated_at";

/// Number of columns in [`QUEST_COLUMNS`]; columns selected after them start at this index.
pub(crate) const QUEST_COLUMN_COUNT: usize = 16;

/// Maps the leading [`QUEST_COLUMNS`] of a row. `prerequisite_ids` is left empty for the caller.
pub(crate) fn map_quest(row: &rusqlite::Row) -> rusqlite::Result<Quest> {
    let created_at: String = row.get(14)?;
    let updated_at: String = row.get(15)?;

    Ok(Quest {
        id: row.get(0)?,
        wiki_page_id: row.get(1)?,
        wiki_title: row.get(2)?,
        name: row.get(3)?,
        localized_name: row.get(4)?,
        source: row.get(5)?,
        quest_type: row.get(6)?,
        region: row.get(7)?,
        recommended_level: row.get(8)?,
        sort_order: row.get(9)?,
        description: row.get(10)?,
        important_notes: row.get(11)?,
        is_unmarked: row.get(12)?,
        cutoff_quest_id: row.get(13)?,
        prerequisite_ids: Vec::new(),
        created_at: parse_timestamp(&created_at, "created_at")?,
        updated_at: parse_timestamp(&updated_at, "updated_at")?,
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
