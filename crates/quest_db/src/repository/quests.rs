use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{QuestTrackerError, Result};
use crate::models::{NewQuest, Quest, QuestFilter, QuestSource};

/// Inserts static quest reference data into the database and returns the generated ID.
/// Also inserts any specified prerequisite relationships.
pub fn insert(conn: &Connection, q: &NewQuest) -> Result<i64> {
    conn.execute(
        "INSERT INTO quests (
            name, source, quest_type, region, recommended_level,
            sort_order, description, is_unmarked, cutoff_quest_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            q.name,
            q.source,
            q.quest_type,
            q.region,
            q.recommended_level,
            q.sort_order,
            q.description,
            q.is_unmarked,
            q.cutoff_quest_id,
        ],
    )?;

    let quest_id = conn.last_insert_rowid();

    for &prereq_id in &q.prerequisite_ids {
        add_prerequisite(conn, quest_id, prereq_id)?;
    }

    Ok(quest_id)
}

/// Retrieves a quest by its ID along with its list of prerequisite quest IDs.
pub fn get(conn: &Connection, id: i64) -> Result<Quest> {
    let mut stmt = conn.prepare(
        "SELECT id, name, source, quest_type, region, recommended_level,
                sort_order, description, is_unmarked, cutoff_quest_id,
                created_at, updated_at
         FROM quests
         WHERE id = ?1",
    )?;

    let mut quest = stmt
        .query_row(params![id], map_row)
        .optional()?
        .ok_or(QuestTrackerError::QuestNotFound(id))?;

    quest.prerequisite_ids = get_prerequisites(conn, id)?;
    Ok(quest)
}

/// Retrieves a quest by its unique combination of name and expansion source.
pub fn get_by_name(conn: &Connection, name: &str, source: &QuestSource) -> Result<Quest> {
    let mut stmt = conn.prepare(
        "SELECT id, name, source, quest_type, region, recommended_level,
                sort_order, description, is_unmarked, cutoff_quest_id,
                created_at, updated_at
         FROM quests
         WHERE name = ?1 AND source = ?2",
    )?;

    let mut quest = stmt
        .query_row(params![name, source], map_row)
        .optional()?
        .ok_or_else(|| QuestTrackerError::QuestNotFoundByName {
            name: name.to_string(),
            source_name: format!("{:?}", source),
        })?;

    quest.prerequisite_ids = get_prerequisites(conn, quest.id)?;
    Ok(quest)
}

/// Queries quests with optional filter criteria and populates prerequisite IDs for each quest.
pub fn list(conn: &Connection, filter: &QuestFilter) -> Result<Vec<Quest>> {
    let mut query = String::from(
        "SELECT id, name, source, quest_type, region, recommended_level,
                sort_order, description, is_unmarked, cutoff_quest_id,
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
        let mut q = row?;
        q.prerequisite_ids = get_prerequisites(conn, q.id)?;
        quests.push(q);
    }
    Ok(quests)
}

/// Links a prerequisite quest to a target quest.
pub fn add_prerequisite(conn: &Connection, quest_id: i64, prerequisite_quest_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO quest_prerequisites (quest_id, prerequisite_quest_id)
         VALUES (?1, ?2)",
        params![quest_id, prerequisite_quest_id],
    )?;
    Ok(())
}

/// Retrieves all prerequisite quest IDs for a given quest ID.
pub fn get_prerequisites(conn: &Connection, quest_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT prerequisite_quest_id
         FROM quest_prerequisites
         WHERE quest_id = ?1
         ORDER BY prerequisite_quest_id ASC",
    )?;

    let rows = stmt.query_map(params![quest_id], |row| row.get(0))?;
    let mut prereqs = Vec::new();
    for row in rows {
        prereqs.push(row?);
    }
    Ok(prereqs)
}

/// Deletes a quest by ID. Deleting a quest cascades to deleting its prerequisite links.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let rows_affected = conn.execute("DELETE FROM quests WHERE id = ?1", params![id])?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Quest> {
    let created_at_str: String = row.get(10)?;
    let updated_at_str: String = row.get(11)?;

    let created_at = parse_timestamp(&created_at_str, "created_at")?;
    let updated_at = parse_timestamp(&updated_at_str, "updated_at")?;

    Ok(Quest {
        id: row.get(0)?,
        name: row.get(1)?,
        source: row.get(2)?,
        quest_type: row.get(3)?,
        region: row.get(4)?,
        recommended_level: row.get(5)?,
        sort_order: row.get(6)?,
        description: row.get(7)?,
        is_unmarked: row.get(8)?,
        cutoff_quest_id: row.get(9)?,
        prerequisite_ids: Vec::new(), // Populated by caller
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
    fn test_quest_crud_cutoff_and_prerequisites() {
        let conn = open_in_memory().unwrap();

        let q1_id = insert(
            &conn,
            &NewQuest {
                name: "Pyres of Novigrad".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::MainQuest,
                region: Region::Novigrad,
                recommended_level: Some(10),
                sort_order: Some(1),
                description: Some("Find Triss Merigold in Novigrad.".into()),
                is_unmarked: false,
                cutoff_quest_id: None,
                prerequisite_ids: vec![],
            },
        )
        .unwrap();

        let q2_id = insert(
            &conn,
            &NewQuest {
                name: "Isle of Mists".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::MainQuest,
                region: Region::Skellige,
                recommended_level: Some(22),
                sort_order: Some(10),
                description: Some("Point of no return.".into()),
                is_unmarked: false,
                cutoff_quest_id: None,
                prerequisite_ids: vec![q1_id],
            },
        )
        .unwrap();

        let q3_id = insert(
            &conn,
            &NewQuest {
                name: "Witch Hunter Raids".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::SecondaryQuest,
                region: Region::Novigrad,
                recommended_level: None,
                sort_order: Some(5),
                description: Some("Unmarked quest.".into()),
                is_unmarked: true,
                cutoff_quest_id: Some(q2_id),
                prerequisite_ids: vec![q1_id],
            },
        )
        .unwrap();

        let q3 = get(&conn, q3_id).unwrap();
        assert_eq!(q3.name, "Witch Hunter Raids");
        assert_eq!(q3.cutoff_quest_id, Some(q2_id));
        assert_eq!(q3.prerequisite_ids, vec![q1_id]);
        assert!(q3.is_unmarked);

        let by_name = get_by_name(&conn, "Witch Hunter Raids", &QuestSource::BaseGame).unwrap();
        assert_eq!(by_name.id, q3_id);
        assert_eq!(by_name.prerequisite_ids, vec![q1_id]);

        let filtered = list(
            &conn,
            &QuestFilter {
                region: Some(Region::Novigrad),
                is_unmarked: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, q3_id);
    }
}
