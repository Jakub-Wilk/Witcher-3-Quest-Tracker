use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, named_params, params};

use super::quests::all_prerequisites;
use super::util::{QUEST_COLUMNS, QUEST_COLUMN_COUNT, QUEST_FROM, map_quest, parse_optional_timestamp};
use crate::error::Result;
use crate::models::{CompletionSummary, Quest, QuestProgress, QuestStatus};

/// SQL expression for the effective status of a `quest_progress` row aliased `qp`.
const EFFECTIVE: &str = "COALESCE(qp.manual_status, qp.save_status, 'NotStarted')";

/// Progress of one quest, if anything was ever recorded for it.
pub fn get(conn: &Connection, playthrough_id: i64, quest_id: i64) -> Result<Option<QuestProgress>> {
    conn.query_row(
        "SELECT playthrough_id, quest_id, manual_status, save_status, notes, completed_at
         FROM quest_progress WHERE playthrough_id = ?1 AND quest_id = ?2",
        params![playthrough_id, quest_id],
        |row| {
            Ok(QuestProgress {
                playthrough_id: row.get(0)?,
                quest_id: row.get(1)?,
                manual_status: row.get(2)?,
                save_status: row.get(3)?,
                notes: row.get(4)?,
                completed_at: parse_optional_timestamp(row.get(5)?, "completed_at")?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

/// Every quest with its progress in a playthrough (default progress where nothing is recorded),
/// in story order, with text in `language`.
pub fn list_for_playthrough(
    conn: &Connection,
    playthrough_id: i64,
    language: &str,
) -> Result<Vec<(Quest, QuestProgress)>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {QUEST_COLUMNS}, qp.manual_status, qp.save_status, qp.notes, qp.completed_at
         {QUEST_FROM}
         LEFT JOIN quest_progress qp ON qp.quest_id = q.id AND qp.playthrough_id = :pt
         ORDER BY q.sort_order ASC, q.base_name ASC"
    ))?;
    let base = QUEST_COLUMN_COUNT;
    let mut results = stmt
        .query_map(named_params! { ":lang": language, ":pt": playthrough_id }, |row| {
            let quest = map_quest(row)?;
            let progress = QuestProgress {
                playthrough_id,
                quest_id: quest.id,
                manual_status: row.get(base)?,
                save_status: row.get(base + 1)?,
                notes: row.get(base + 2)?,
                completed_at: parse_optional_timestamp(row.get(base + 3)?, "completed_at")?,
            };
            Ok((quest, progress))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut prereqs = all_prerequisites(conn)?;
    for (quest, _) in &mut results {
        quest.prerequisite_ids = prereqs.remove(&quest.id).unwrap_or_default();
    }
    Ok(results)
}

/// Sets or clears (`None`: follow the save again) the user's status for a quest.
pub fn set_manual_status(
    conn: &Connection,
    playthrough_id: i64,
    quest_id: i64,
    status: Option<QuestStatus>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO quest_progress (playthrough_id, quest_id, manual_status) VALUES (?1, ?2, ?3)
         ON CONFLICT(playthrough_id, quest_id) DO UPDATE SET manual_status = EXCLUDED.manual_status",
        params![playthrough_id, quest_id, status],
    )?;
    refresh_completed_at(conn, playthrough_id, Some(quest_id))
}

/// Sets or clears the user's notes for a quest.
pub fn set_notes(
    conn: &Connection,
    playthrough_id: i64,
    quest_id: i64,
    notes: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO quest_progress (playthrough_id, quest_id, notes) VALUES (?1, ?2, ?3)
         ON CONFLICT(playthrough_id, quest_id) DO UPDATE SET notes = EXCLUDED.notes",
        params![playthrough_id, quest_id, notes],
    )?;
    Ok(())
}

/// Outcome of [`apply_save_statuses`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SaveApplyReport {
    /// Quests whose save status changed.
    pub changed: usize,
    /// Quests whose save status went backwards (e.g. Completed to InProgress), as after loading
    /// an older save.
    pub reverted: Vec<i64>,
}

/// Replaces the save-derived status of every quest in a playthrough: quests in `statuses` get
/// that status, all others `NotStarted`. Manual statuses and notes are untouched. Run it inside
/// a transaction.
pub fn apply_save_statuses(
    conn: &Connection,
    playthrough_id: i64,
    statuses: &HashMap<i64, QuestStatus>,
) -> Result<SaveApplyReport> {
    let previous: HashMap<i64, Option<QuestStatus>> = {
        let mut stmt = conn.prepare(
            "SELECT q.id, qp.save_status FROM quests q
             LEFT JOIN quest_progress qp ON qp.quest_id = q.id AND qp.playthrough_id = ?1",
        )?;
        let rows = stmt.query_map(params![playthrough_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    let mut report = SaveApplyReport::default();
    let mut stmt = conn.prepare(
        "INSERT INTO quest_progress (playthrough_id, quest_id, save_status) VALUES (?1, ?2, ?3)
         ON CONFLICT(playthrough_id, quest_id) DO UPDATE SET save_status = EXCLUDED.save_status",
    )?;
    for (&quest_id, &old) in &previous {
        let new = statuses.get(&quest_id).copied().unwrap_or(QuestStatus::NotStarted);
        if old == Some(new) {
            continue;
        }
        stmt.execute(params![playthrough_id, quest_id, new])?;
        report.changed += 1;
        if old.is_some_and(|old| rank(new) < rank(old)) {
            report.reverted.push(quest_id);
        }
    }
    report.reverted.sort_unstable();
    refresh_completed_at(conn, playthrough_id, None)?;
    Ok(report)
}

/// Clears the user's statuses on quests saves can report (journal quests), so a playthrough
/// newly linked to its saves shows exactly what the saves say. Notes stay.
pub fn clear_manual_statuses_of_trackable(conn: &Connection, playthrough_id: i64) -> Result<()> {
    conn.execute(
        &format!(
            "UPDATE quest_progress SET manual_status = NULL
             WHERE playthrough_id = ?1 AND quest_id IN
                (SELECT id FROM quests WHERE journal_path NOT LIKE '{}%')",
            crate::models::WIKI_ONLY_PREFIX
        ),
        params![playthrough_id],
    )?;
    refresh_completed_at(conn, playthrough_id, None)
}

/// Forgets all save-derived statuses of a playthrough (when it is unlinked from its saves).
pub fn clear_save_statuses(conn: &Connection, playthrough_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE quest_progress SET save_status = NULL WHERE playthrough_id = ?1",
        params![playthrough_id],
    )?;
    refresh_completed_at(conn, playthrough_id, None)
}

/// How far along a status is; used to detect statuses going backwards.
fn rank(status: QuestStatus) -> u8 {
    match status {
        QuestStatus::NotStarted => 0,
        QuestStatus::InProgress => 1,
        QuestStatus::Completed | QuestStatus::Failed => 2,
    }
}

/// Stamps `completed_at` on rows whose effective status just became Completed and clears it on
/// rows that are no longer Completed.
fn refresh_completed_at(conn: &Connection, playthrough_id: i64, quest_id: Option<i64>) -> Result<()> {
    conn.execute(
        &format!(
            "UPDATE quest_progress AS qp SET completed_at =
                CASE WHEN {EFFECTIVE} = 'Completed' THEN COALESCE(qp.completed_at, ?3) ELSE NULL END
             WHERE qp.playthrough_id = ?1 AND (?2 IS NULL OR qp.quest_id = ?2)"
        ),
        params![playthrough_id, quest_id, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// Aggregate completion counts for a playthrough, by effective status.
pub fn completion_summary(conn: &Connection, playthrough_id: i64) -> Result<CompletionSummary> {
    let sql = format!(
        "SELECT
            COUNT(q.id),
            COALESCE(SUM({EFFECTIVE} = 'NotStarted'), 0),
            COALESCE(SUM({EFFECTIVE} = 'InProgress'), 0),
            COALESCE(SUM({EFFECTIVE} = 'Completed'), 0),
            COALESCE(SUM({EFFECTIVE} = 'Failed'), 0)
         FROM quests q
         LEFT JOIN quest_progress qp ON q.id = qp.quest_id AND qp.playthrough_id = ?1"
    );
    conn.query_row(&sql, params![playthrough_id], |row| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::NewPlaythrough;
    use crate::repository::{playthroughs, quests};

    fn playthrough(conn: &Connection) -> i64 {
        playthroughs::insert(
            conn,
            &NewPlaythrough {
                name: "Run".into(),
                is_new_game_plus: false,
                notes: None,
                link: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn summary_with_no_quests_is_all_zero() {
        // SUM over zero rows is NULL in SQL; a fresh DB (no sync yet) must still work.
        let conn = open_in_memory().unwrap();
        let pt = playthrough(&conn);
        assert_eq!(completion_summary(&conn, pt).unwrap(), CompletionSummary::default());
    }

    #[test]
    fn manual_status_wins_over_save_status() {
        let conn = open_in_memory().unwrap();
        let pt = playthrough(&conn);
        let (a, _) = quests::upsert(&conn, &quests::tests::sample("Lilac and Gooseberries", 1)).unwrap();
        let (b, _) = quests::upsert(&conn, &quests::tests::sample("Devil by the Well", 2)).unwrap();

        apply_save_statuses(&conn, pt, &HashMap::from([(a, QuestStatus::InProgress)])).unwrap();
        set_manual_status(&conn, pt, a, Some(QuestStatus::Completed)).unwrap();
        let p = get(&conn, pt, a).unwrap().unwrap();
        assert_eq!(p.status(), QuestStatus::Completed);
        assert!(p.overrides_save());
        assert!(p.completed_at.is_some());

        // Clearing the manual status follows the save again.
        set_manual_status(&conn, pt, a, None).unwrap();
        let p = get(&conn, pt, a).unwrap().unwrap();
        assert_eq!(p.status(), QuestStatus::InProgress);
        assert!(p.completed_at.is_none());

        let summary = completion_summary(&conn, pt).unwrap();
        assert_eq!((summary.total_quests, summary.in_progress, summary.not_started), (2, 1, 1));

        let list = list_for_playthrough(&conn, pt, "en").unwrap();
        assert_eq!(list.len(), 2);
        let row_b = list.iter().find(|(q, _)| q.id == b).unwrap();
        assert_eq!(row_b.1.status(), QuestStatus::NotStarted);
    }

    #[test]
    fn applying_saves_reports_changes_and_reverts() {
        let conn = open_in_memory().unwrap();
        let pt = playthrough(&conn);
        let (a, _) = quests::upsert(&conn, &quests::tests::sample("A", 1)).unwrap();
        let (b, _) = quests::upsert(&conn, &quests::tests::sample("B", 2)).unwrap();

        let first = apply_save_statuses(
            &conn,
            pt,
            &HashMap::from([(a, QuestStatus::Completed), (b, QuestStatus::InProgress)]),
        )
        .unwrap();
        assert_eq!(first, SaveApplyReport { changed: 2, reverted: vec![] });
        assert!(get(&conn, pt, a).unwrap().unwrap().completed_at.is_some());

        // Same state again: nothing changes.
        let same = apply_save_statuses(
            &conn,
            pt,
            &HashMap::from([(a, QuestStatus::Completed), (b, QuestStatus::InProgress)]),
        )
        .unwrap();
        assert_eq!(same, SaveApplyReport::default());

        // An older save: A is back in progress, B not started yet.
        let older = apply_save_statuses(&conn, pt, &HashMap::from([(a, QuestStatus::InProgress)])).unwrap();
        assert_eq!(older, SaveApplyReport { changed: 2, reverted: vec![a, b] });
        assert!(get(&conn, pt, a).unwrap().unwrap().completed_at.is_none());

        clear_save_statuses(&conn, pt).unwrap();
        assert_eq!(get(&conn, pt, a).unwrap().unwrap().status(), QuestStatus::NotStarted);
    }

    #[test]
    fn notes_do_not_touch_status() {
        let conn = open_in_memory().unwrap();
        let pt = playthrough(&conn);
        let (a, _) = quests::upsert(&conn, &quests::tests::sample("A", 1)).unwrap();
        set_notes(&conn, pt, a, Some("Bring a crossbow")).unwrap();
        let p = get(&conn, pt, a).unwrap().unwrap();
        assert_eq!(p.notes.as_deref(), Some("Bring a crossbow"));
        assert_eq!(p.manual_status, None);
        set_notes(&conn, pt, a, None).unwrap();
        assert_eq!(get(&conn, pt, a).unwrap().unwrap().notes, None);
    }
}
