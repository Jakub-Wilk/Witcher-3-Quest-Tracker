use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{QuestTrackerError, Result};
use crate::models::{NewQuest, Quest, QuestFilter, QuestSource};

/// Inserts static quest reference data into the database and returns the generated ID.
pub fn insert(conn: &Connection, q: &NewQuest) -> Result<i64> {
    conn.execute(
        "INSERT INTO quests (
            name, source, quest_type, region, recommended_level,
            is_failable, sort_order, description, is_unmarked, cutoff_quest_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            q.name,
            q.source,
            q.quest_type,
            q.region,
            q.recommended_level,
            q.is_failable,
            q.sort_order,
            q.description,
            q.is_unmarked,
            q.cutoff_quest_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Retrieves a quest by its ID. Returns `QuestTrackerError::QuestNotFound` if missing.
pub fn get(conn: &Connection, id: i64) -> Result<Quest> {
    let mut stmt = conn.prepare(
        "SELECT id, name, source, quest_type, region, recommended_level,
                is_failable, sort_order, description, is_unmarked, cutoff_quest_id,
                created_at, updated_at
         FROM quests
         WHERE id = ?1",
    )?;

    stmt.query_row(params![id], map_row)
        .optional()?
        .ok_or(QuestTrackerError::QuestNotFound(id))
}

/// Retrieves a quest by its unique combination of name and expansion source.
pub fn get_by_name(conn: &Connection, name: &str, source: &QuestSource) -> Result<Quest> {
    let mut stmt = conn.prepare(
        "SELECT id, name, source, quest_type, region, recommended_level,
                is_failable, sort_order, description, is_unmarked, cutoff_quest_id,
                created_at, updated_at
         FROM quests
         WHERE name = ?1 AND source = ?2",
    )?;

    stmt.query_row(params![name, source], map_row)
        .optional()?
        .ok_or_else(|| QuestTrackerError::QuestNotFoundByName {
            name: name.to_string(),
            source_name: format!("{:?}", source),
        })
}

/// Queries quests with optional filter criteria.
pub fn list(conn: &Connection, filter: &QuestFilter) -> Result<Vec<Quest>> {
    let mut query = String::from(
        "SELECT id, name, source, quest_type, region, recommended_level,
                is_failable, sort_order, description, is_unmarked, cutoff_quest_id,
                created_at, updated_at
         FROM quests
         WHERE 1=1",
    );

    let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref source) = filter.source {
        param_values.push(Box::new(*source));
        query.push_str(&format!(" AND source = ?{}", param_values.len()));
    }
    if let Some(ref quest_type) = filter.quest_type {
        param_values.push(Box::new(*quest_type));
        query.push_str(&format!(" AND quest_type = ?{}", param_values.len()));
    }
    if let Some(ref region) = filter.region {
        param_values.push(Box::new(*region));
        query.push_str(&format!(" AND region = ?{}", param_values.len()));
    }
    if let Some(is_failable) = filter.is_failable {
        param_values.push(Box::new(is_failable));
        query.push_str(&format!(" AND is_failable = ?{}", param_values.len()));
    }
    if let Some(is_unmarked) = filter.is_unmarked {
        param_values.push(Box::new(is_unmarked));
        query.push_str(&format!(" AND is_unmarked = ?{}", param_values.len()));
    }
    if let Some(max_lvl) = filter.max_recommended_level {
        param_values.push(Box::new(max_lvl));
        query.push_str(&format!(
            " AND (recommended_level IS NULL OR recommended_level <= ?{})",
            param_values.len()
        ));
    }

    query.push_str(" ORDER BY sort_order ASC, name ASC");

    let mut stmt = conn.prepare(&query)?;
    let params_slice: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(params_slice.as_slice(), map_row)?;
    let mut quests = Vec::new();
    for row in rows {
        quests.push(row?);
    }
    Ok(quests)
}

/// Deletes a quest by ID. Returns error if quest does not exist.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let rows_affected = conn.execute("DELETE FROM quests WHERE id = ?1", params![id])?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Quest> {
    let created_at_str: String = row.get(11)?;
    let updated_at_str: String = row.get(12)?;

    let created_at = parse_timestamp(&created_at_str, "created_at")?;
    let updated_at = parse_timestamp(&updated_at_str, "updated_at")?;

    Ok(Quest {
        id: row.get(0)?,
        name: row.get(1)?,
        source: row.get(2)?,
        quest_type: row.get(3)?,
        region: row.get(4)?,
        recommended_level: row.get(5)?,
        is_failable: row.get(6)?,
        sort_order: row.get(7)?,
        description: row.get(8)?,
        is_unmarked: row.get(9)?,
        cutoff_quest_id: row.get(10)?,
        created_at,
        updated_at,
    })
}

fn parse_timestamp(s: &str, field_name: &str) -> rusqlite::Result<DateTime<Utc>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{QuestType, Region};

    #[test]
    fn test_quest_crud_and_cutoff() {
        let conn = open_in_memory().unwrap();

        let q1_id = insert(
            &conn,
            &NewQuest {
                name: "Isle of Mists".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::MainQuest,
                region: Region::Skellige,
                recommended_level: Some(22),
                is_failable: false,
                sort_order: Some(10),
                description: Some("Point of no return for many secondary quests.".into()),
                is_unmarked: false,
                cutoff_quest_id: None,
            },
        )
        .unwrap();

        let q2_id = insert(
            &conn,
            &NewQuest {
                name: "The Last Wish".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::SecondaryQuest,
                region: Region::Skellige,
                recommended_level: Some(15),
                is_failable: true,
                sort_order: Some(5),
                description: Some("Yennefer's romance quest.".into()),
                is_unmarked: false,
                cutoff_quest_id: Some(q1_id),
            },
        )
        .unwrap();

        let q2 = get(&conn, q2_id).unwrap();
        assert_eq!(q2.name, "The Last Wish");
        assert_eq!(q2.cutoff_quest_id, Some(q1_id));
        assert!(q2.is_failable);
        assert!(!q2.is_unmarked);

        let by_name = get_by_name(&conn, "The Last Wish", &QuestSource::BaseGame).unwrap();
        assert_eq!(by_name.id, q2_id);

        let filtered = list(
            &conn,
            &QuestFilter {
                region: Some(Region::Skellige),
                is_failable: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, q2_id);
    }
}
