use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{QuestTrackerError, Result};
use crate::models::{
    CompletionSummary, NewQuestProgress, ProgressFilter, Quest, QuestProgress,
    QuestProgressUpdate, QuestStatus,
};

/// Inserts or updates progress for a quest within a playthrough.
pub fn upsert(conn: &Connection, p: &NewQuestProgress) -> Result<()> {
    conn.execute(
        "INSERT INTO quest_progress (
            playthrough_id, quest_id, status, notes, started_at, completed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(playthrough_id, quest_id) DO UPDATE SET
            status = EXCLUDED.status,
            notes = EXCLUDED.notes,
            started_at = EXCLUDED.started_at,
            completed_at = EXCLUDED.completed_at",
        params![
            p.playthrough_id,
            p.quest_id,
            p.status,
            p.notes,
            p.started_at,
            p.completed_at,
        ],
    )?;
    Ok(())
}

/// Retrieves progress for a specific quest in a specific playthrough.
/// Returns `QuestTrackerError::ProgressNotFound` if no row exists yet.
pub fn get(conn: &Connection, playthrough_id: i64, quest_id: i64) -> Result<QuestProgress> {
    let mut stmt = conn.prepare(
        "SELECT id, playthrough_id, quest_id, status, notes, started_at, completed_at,
                created_at, updated_at
         FROM quest_progress
         WHERE playthrough_id = ?1 AND quest_id = ?2",
    )?;

    stmt.query_row(params![playthrough_id, quest_id], map_progress_row)
        .optional()?
        .ok_or(QuestTrackerError::ProgressNotFound {
            playthrough_id,
            quest_id,
        })
}

/// Lists all quests and their progress for a given playthrough matching the filter criteria.
/// Quests without an explicit progress record are reported as `NotStarted`.
pub fn list_for_playthrough(
    conn: &Connection,
    playthrough_id: i64,
    filter: &ProgressFilter,
) -> Result<Vec<(Quest, QuestProgress)>> {
    let mut query = String::from(
        "SELECT q.id, q.name, q.source, q.quest_type, q.region, q.recommended_level,
                q.sort_order, q.description, q.is_unmarked, q.cutoff_quest_id,
                q.created_at, q.updated_at,
                qp.id, qp.playthrough_id, qp.quest_id, COALESCE(qp.status, 'NotStarted'),
                qp.notes, qp.started_at, qp.completed_at, qp.created_at, qp.updated_at
         FROM quests q
         LEFT JOIN quest_progress qp ON q.id = qp.quest_id AND qp.playthrough_id = ?1
         WHERE 1=1",
    );

    let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(playthrough_id)];

    if let Some(ref status) = filter.status {
        param_values.push(Box::new(*status));
        query.push_str(&format!(
            " AND COALESCE(qp.status, 'NotStarted') = ?{}",
            param_values.len()
        ));
    }
    if let Some(ref source) = filter.source {
        param_values.push(Box::new(*source));
        query.push_str(&format!(" AND q.source = ?{}", param_values.len()));
    }
    if let Some(ref quest_type) = filter.quest_type {
        param_values.push(Box::new(*quest_type));
        query.push_str(&format!(" AND q.quest_type = ?{}", param_values.len()));
    }
    if let Some(ref region) = filter.region {
        param_values.push(Box::new(*region));
        query.push_str(&format!(" AND q.region = ?{}", param_values.len()));
    }

    query.push_str(" ORDER BY q.sort_order ASC, q.name ASC");

    let mut stmt = conn.prepare(&query)?;
    let params_slice: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(params_slice.as_slice(), |row| {
        let quest = map_quest_part(row)?;
        let progress = map_progress_part(row, playthrough_id, quest.id)?;
        Ok((quest, progress))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (mut quest, progress) = row?;
        quest.prerequisite_ids = crate::repository::quests::get_prerequisites(conn, quest.id)?;
        results.push((quest, progress));
    }
    Ok(results)
}

/// Partially updates progress for a quest in a playthrough. Creates a progress entry if none exists.
pub fn update_status(
    conn: &Connection,
    playthrough_id: i64,
    quest_id: i64,
    update: &QuestProgressUpdate,
) -> Result<()> {
    let current = get(conn, playthrough_id, quest_id).ok();

    let new_status = update.status.unwrap_or_else(|| {
        current
            .as_ref()
            .map(|c| c.status)
            .unwrap_or(QuestStatus::NotStarted)
    });

    let new_notes = match &update.notes {
        Some(inner) => inner.clone(),
        None => current.as_ref().and_then(|c| c.notes.clone()),
    };

    let new_started_at = match &update.started_at {
        Some(inner) => *inner,
        None => current.as_ref().and_then(|c| c.started_at),
    };

    let new_completed_at = match &update.completed_at {
        Some(inner) => *inner,
        None => current.as_ref().and_then(|c| c.completed_at),
    };

    upsert(
        conn,
        &NewQuestProgress {
            playthrough_id,
            quest_id,
            status: new_status,
            notes: new_notes,
            started_at: new_started_at,
            completed_at: new_completed_at,
        },
    )
}

/// Generates aggregate completion stats for a given playthrough.
pub fn completion_summary(conn: &Connection, playthrough_id: i64) -> Result<CompletionSummary> {
    let mut stmt = conn.prepare(
        "SELECT
            COUNT(q.id) AS total_quests,
            SUM(CASE WHEN COALESCE(qp.status, 'NotStarted') = 'NotStarted' THEN 1 ELSE 0 END) AS not_started,
            SUM(CASE WHEN qp.status = 'InProgress' THEN 1 ELSE 0 END) AS in_progress,
            SUM(CASE WHEN qp.status = 'Completed' THEN 1 ELSE 0 END) AS completed,
            SUM(CASE WHEN qp.status = 'Failed' THEN 1 ELSE 0 END) AS failed
         FROM quests q
         LEFT JOIN quest_progress qp ON q.id = qp.quest_id AND qp.playthrough_id = ?1",
    )?;

    stmt.query_row(params![playthrough_id], |row| {
        Ok(CompletionSummary {
            total_quests: row.get(0)?,
            not_started: row.get(1)?,
            in_progress: row.get(2)?,
            completed: row.get(3)?,
            failed: row.get(4)?,
        })
    })
    .map_err(Into::into)
}

fn map_progress_row(row: &rusqlite::Row) -> rusqlite::Result<QuestProgress> {
    let created_at_str: String = row.get(7)?;
    let updated_at_str: String = row.get(8)?;

    let created_at = parse_timestamp(&created_at_str, "created_at")?;
    let updated_at = parse_timestamp(&updated_at_str, "updated_at")?;

    let started_at: Option<String> = row.get(5)?;
    let completed_at: Option<String> = row.get(6)?;

    Ok(QuestProgress {
        id: row.get(0)?,
        playthrough_id: row.get(1)?,
        quest_id: row.get(2)?,
        status: row.get(3)?,
        notes: row.get(4)?,
        started_at: started_at
            .as_deref()
            .map(|s| parse_timestamp(s, "started_at"))
            .transpose()?,
        completed_at: completed_at
            .as_deref()
            .map(|s| parse_timestamp(s, "completed_at"))
            .transpose()?,
        created_at,
        updated_at,
    })
}

fn map_quest_part(row: &rusqlite::Row) -> rusqlite::Result<Quest> {
    let created_at_str: String = row.get(10)?;
    let updated_at_str: String = row.get(11)?;

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
        prerequisite_ids: Vec::new(),
        created_at: parse_timestamp(&created_at_str, "created_at")?,
        updated_at: parse_timestamp(&updated_at_str, "updated_at")?,
    })
}

fn map_progress_part(
    row: &rusqlite::Row,
    playthrough_id: i64,
    quest_id: i64,
) -> rusqlite::Result<QuestProgress> {
    let progress_id: Option<i64> = row.get(12)?;

    let status: QuestStatus = row.get(15)?;
    let notes: Option<String> = row.get(16)?;
    let started_at_str: Option<String> = row.get(17)?;
    let completed_at_str: Option<String> = row.get(18)?;

    let created_at_str: Option<String> = row.get(19)?;
    let updated_at_str: Option<String> = row.get(20)?;

    let now = Utc::now();
    let created_at = created_at_str
        .as_deref()
        .map(|s| parse_timestamp(s, "created_at"))
        .transpose()?
        .unwrap_or(now);

    let updated_at = updated_at_str
        .as_deref()
        .map(|s| parse_timestamp(s, "updated_at"))
        .transpose()?
        .unwrap_or(now);

    Ok(QuestProgress {
        id: progress_id.unwrap_or(0),
        playthrough_id,
        quest_id,
        status,
        notes,
        started_at: started_at_str
            .as_deref()
            .map(|s| parse_timestamp(s, "started_at"))
            .transpose()?,
        completed_at: completed_at_str
            .as_deref()
            .map(|s| parse_timestamp(s, "completed_at"))
            .transpose()?,
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
    use crate::models::{Difficulty, NewPlaythrough, NewQuest, QuestSource, QuestType, Region};
    use crate::repository::{playthroughs, quests};

    #[test]
    fn test_progress_tracking_and_summary() {
        let conn = open_in_memory().unwrap();

        let pt_id = playthroughs::insert(
            &conn,
            &NewPlaythrough {
                name: "Playthrough 1".into(),
                difficulty: Difficulty::StoryAndSword,
                is_new_game_plus: false,
                notes: None,
            },
        )
        .unwrap();

        let q1_id = quests::insert(
            &conn,
            &NewQuest {
                name: "Lilac and Gooseberries".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::MainQuest,
                region: Region::WhiteOrchard,
                recommended_level: Some(1),
                sort_order: Some(1),
                description: None,
                is_unmarked: false,
                cutoff_quest_id: None,
                prerequisite_ids: vec![],
            },
        )
        .unwrap();

        let q2_id = quests::insert(
            &conn,
            &NewQuest {
                name: "Devil by the Well".into(),
                source: QuestSource::BaseGame,
                quest_type: QuestType::WitcherContract,
                region: Region::WhiteOrchard,
                recommended_level: Some(2),
                sort_order: Some(2),
                description: None,
                is_unmarked: false,
                cutoff_quest_id: None,
                prerequisite_ids: vec![],
            },
        )
        .unwrap();

        // Initial summary check
        let summary1 = completion_summary(&conn, pt_id).unwrap();
        assert_eq!(summary1.total_quests, 2);
        assert_eq!(summary1.not_started, 2);
        assert_eq!(summary1.completed, 0);

        // Complete quest 1
        upsert(
            &conn,
            &NewQuestProgress {
                playthrough_id: pt_id,
                quest_id: q1_id,
                status: QuestStatus::Completed,
                notes: Some("Met Yennefer".into()),
                started_at: Some(Utc::now()),
                completed_at: Some(Utc::now()),
            },
        )
        .unwrap();

        // Start quest 2
        update_status(
            &conn,
            pt_id,
            q2_id,
            &QuestProgressUpdate {
                status: Some(QuestStatus::InProgress),
                notes: Some(Some("Found the bracelet".into())),
                ..Default::default()
            },
        )
        .unwrap();

        let summary2 = completion_summary(&conn, pt_id).unwrap();
        assert_eq!(summary2.total_quests, 2);
        assert_eq!(summary2.not_started, 0);
        assert_eq!(summary2.in_progress, 1);
        assert_eq!(summary2.completed, 1);

        let list = list_for_playthrough(
            &conn,
            pt_id,
            &ProgressFilter {
                status: Some(QuestStatus::Completed),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0.id, q1_id);
        assert_eq!(list[0].1.status, QuestStatus::Completed);
    }
}
