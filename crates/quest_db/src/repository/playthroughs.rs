use rusqlite::{Connection, OptionalExtension, params};

use super::util::parse_timestamp;
use crate::error::{QuestTrackerError, Result};
use crate::models::{HeadSave, NewPlaythrough, Playthrough, PlaythroughUpdate, SaveLink};

const COLUMNS: &str = "id, name, difficulty, is_new_game_plus, notes, lineage_root,
    game_playthrough_id, started_at, head_save_key, head_save_file, head_saved_at, created_at,
    updated_at";

/// Inserts a new playthrough and returns its id.
pub fn insert(conn: &Connection, p: &NewPlaythrough) -> Result<i64> {
    conn.execute(
        "INSERT INTO playthroughs (name, difficulty, is_new_game_plus, notes, lineage_root,
                                   game_playthrough_id, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            p.name,
            p.difficulty,
            p.is_new_game_plus,
            p.notes,
            p.link.as_ref().map(|l| l.lineage_root),
            p.link.as_ref().and_then(|l| l.game_playthrough_id.clone()),
            p.link.as_ref().and_then(|l| l.started_at),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Retrieves a playthrough by its ID. Returns `QuestTrackerError::PlaythroughNotFound` if missing.
pub fn get(conn: &Connection, id: i64) -> Result<Playthrough> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM playthroughs WHERE id = ?1"), params![id], map_row)
        .optional()?
        .ok_or(QuestTrackerError::PlaythroughNotFound(id))
}

/// Lists all playthroughs ordered by ID.
pub fn list(conn: &Connection) -> Result<Vec<Playthrough>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM playthroughs ORDER BY id ASC"))?;
    let rows = stmt.query_map([], map_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The playthrough linked to a save lineage, if any.
pub fn find_by_lineage(conn: &Connection, lineage_root: i64) -> Result<Option<Playthrough>> {
    conn.query_row(
        &format!("SELECT {COLUMNS} FROM playthroughs WHERE lineage_root = ?1"),
        params![lineage_root],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

/// Updates an existing playthrough. Returns error if playthrough is not found.
pub fn update(conn: &Connection, id: i64, update: &PlaythroughUpdate) -> Result<()> {
    let current = get(conn, id)?;
    let new_name = update.name.as_ref().unwrap_or(&current.name);
    let new_difficulty = update.difficulty.unwrap_or(current.difficulty);
    let new_is_ngp = update.is_new_game_plus.unwrap_or(current.is_new_game_plus);
    let new_notes = match &update.notes {
        Some(inner) => inner.clone(),
        None => current.notes,
    };
    conn.execute(
        "UPDATE playthroughs
         SET name = ?1, difficulty = ?2, is_new_game_plus = ?3, notes = ?4
         WHERE id = ?5",
        params![new_name, new_difficulty, new_is_ngp, new_notes, id],
    )?;
    Ok(())
}

/// Links a playthrough to a save lineage (or unlinks it with `None`). Changing the link forgets
/// the head save.
pub fn set_link(conn: &Connection, id: i64, link: Option<&SaveLink>) -> Result<()> {
    let rows_affected = conn.execute(
        "UPDATE playthroughs
         SET lineage_root = ?2, game_playthrough_id = ?3, started_at = ?4,
             head_save_key = NULL, head_save_file = NULL, head_saved_at = NULL
         WHERE id = ?1",
        params![
            id,
            link.map(|l| l.lineage_root),
            link.and_then(|l| l.game_playthrough_id.clone()),
            link.and_then(|l| l.started_at),
        ],
    )?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::PlaythroughNotFound(id));
    }
    Ok(())
}

/// Records the newest save applied to a playthrough.
pub fn set_head(conn: &Connection, id: i64, head: &HeadSave) -> Result<()> {
    let rows_affected = conn.execute(
        "UPDATE playthroughs SET head_save_key = ?2, head_save_file = ?3, head_saved_at = ?4
         WHERE id = ?1",
        params![id, head.key, head.file, head.saved_at],
    )?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::PlaythroughNotFound(id));
    }
    Ok(())
}

/// Deletes a playthrough by ID. Deleting a playthrough cascades to deleting all progress rows.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let rows_affected = conn.execute("DELETE FROM playthroughs WHERE id = ?1", params![id])?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::PlaythroughNotFound(id));
    }
    Ok(())
}

/// Stops offering to track a save lineage.
pub fn ignore_lineage(conn: &Connection, lineage_root: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO ignored_lineages (lineage_root) VALUES (?1)",
        params![lineage_root],
    )?;
    Ok(())
}

pub fn is_lineage_ignored(conn: &Connection, lineage_root: i64) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM ignored_lineages WHERE lineage_root = ?1)",
        params![lineage_root],
        |row| row.get(0),
    )?)
}

/// Offers ignored lineages again.
pub fn clear_ignored_lineages(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM ignored_lineages", [])?;
    Ok(())
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Playthrough> {
    let lineage_root: Option<i64> = row.get(5)?;
    let head_key: Option<i64> = row.get(8)?;
    let created_at: String = row.get(11)?;
    let updated_at: String = row.get(12)?;
    Ok(Playthrough {
        id: row.get(0)?,
        name: row.get(1)?,
        difficulty: row.get(2)?,
        is_new_game_plus: row.get(3)?,
        notes: row.get(4)?,
        link: match lineage_root {
            Some(lineage_root) => Some(SaveLink {
                lineage_root,
                game_playthrough_id: row.get(6)?,
                started_at: row.get(7)?,
            }),
            None => None,
        },
        head: match head_key {
            Some(key) => Some(HeadSave {
                key,
                file: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                saved_at: row.get(10)?,
            }),
            None => None,
        },
        created_at: parse_timestamp(&created_at, "created_at")?,
        updated_at: parse_timestamp(&updated_at, "updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::db::open_in_memory;
    use crate::models::Difficulty;

    fn new(name: &str, link: Option<SaveLink>) -> NewPlaythrough {
        NewPlaythrough {
            name: name.into(),
            difficulty: Difficulty::DeathMarch,
            is_new_game_plus: false,
            notes: Some("Planning to 100% all Gwent cards".into()),
            link,
        }
    }

    #[test]
    fn playthrough_crud() {
        let conn = open_in_memory().unwrap();
        let id = insert(&conn, &new("Death March Run 1", None)).unwrap();
        let p = get(&conn, id).unwrap();
        assert_eq!(p.name, "Death March Run 1");
        assert_eq!(p.link, None);
        assert_eq!(p.head, None);

        update(
            &conn,
            id,
            &PlaythroughUpdate {
                name: Some("Death March NGP".into()),
                is_new_game_plus: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let p = get(&conn, id).unwrap();
        assert_eq!(p.name, "Death March NGP");
        assert!(p.is_new_game_plus);
        assert_eq!(list(&conn).unwrap().len(), 1);

        delete(&conn, id).unwrap();
        assert!(get(&conn, id).is_err());
    }

    #[test]
    fn lineage_link_and_head() {
        let conn = open_in_memory().unwrap();
        let started = NaiveDate::from_ymd_opt(2026, 10, 2).unwrap().and_hms_milli_opt(15, 35, 21, 269).unwrap();
        let link = SaveLink {
            lineage_root: 0x7ea4_8400_03e3_550d,
            game_playthrough_id: Some("6abfb2520000423b".into()),
            started_at: Some(started),
        };
        let id = insert(&conn, &new("Remastered", Some(link.clone()))).unwrap();
        assert_eq!(find_by_lineage(&conn, link.lineage_root).unwrap().unwrap().id, id);
        assert_eq!(get(&conn, id).unwrap().link, Some(link.clone()));

        // A lineage links to at most one playthrough.
        assert!(insert(&conn, &new("Duplicate", Some(link.clone()))).is_err());

        let head = HeadSave { key: 0x7ea4_9400_0027_df70, file: "ManualSave_x.sav".into(), saved_at: Some(started) };
        set_head(&conn, id, &head).unwrap();
        assert_eq!(get(&conn, id).unwrap().head, Some(head));

        set_link(&conn, id, None).unwrap();
        let p = get(&conn, id).unwrap();
        assert_eq!((p.link, p.head), (None, None));
        assert!(find_by_lineage(&conn, link.lineage_root).unwrap().is_none());
    }

    #[test]
    fn ignored_lineages() {
        let conn = open_in_memory().unwrap();
        assert!(!is_lineage_ignored(&conn, 5).unwrap());
        ignore_lineage(&conn, 5).unwrap();
        ignore_lineage(&conn, 5).unwrap();
        assert!(is_lineage_ignored(&conn, 5).unwrap());
        clear_ignored_lineages(&conn).unwrap();
        assert!(!is_lineage_ignored(&conn, 5).unwrap());
    }
}
