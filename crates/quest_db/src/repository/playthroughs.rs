use rusqlite::{params, Connection, OptionalExtension};

use super::util::parse_timestamp;
use crate::error::{QuestTrackerError, Result};
use crate::models::{NewPlaythrough, Playthrough, PlaythroughUpdate};

/// Inserts a new playthrough record into the database and returns the generated ID.
pub fn insert(conn: &Connection, p: &NewPlaythrough) -> Result<i64> {
    conn.execute(
        "INSERT INTO playthroughs (name, difficulty, is_new_game_plus, notes)
         VALUES (?1, ?2, ?3, ?4)",
        params![p.name, p.difficulty, p.is_new_game_plus, p.notes],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Retrieves a playthrough by its ID. Returns `QuestTrackerError::PlaythroughNotFound` if missing.
pub fn get(conn: &Connection, id: i64) -> Result<Playthrough> {
    let mut stmt = conn.prepare(
        "SELECT id, name, difficulty, is_new_game_plus, notes, created_at, updated_at
         FROM playthroughs
         WHERE id = ?1",
    )?;

    stmt.query_row(params![id], map_row)
        .optional()?
        .ok_or(QuestTrackerError::PlaythroughNotFound(id))
}

/// Lists all playthroughs ordered by ID.
pub fn list(conn: &Connection) -> Result<Vec<Playthrough>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, difficulty, is_new_game_plus, notes, created_at, updated_at
         FROM playthroughs
         ORDER BY id ASC",
    )?;

    let rows = stmt.query_map([], map_row)?;
    let mut playthroughs = Vec::new();
    for row in rows {
        playthroughs.push(row?);
    }
    Ok(playthroughs)
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

    let rows_affected = conn.execute(
        "UPDATE playthroughs
         SET name = ?1, difficulty = ?2, is_new_game_plus = ?3, notes = ?4
         WHERE id = ?5",
        params![new_name, new_difficulty, new_is_ngp, new_notes, id],
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

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Playthrough> {
    let created_at_str: String = row.get(5)?;
    let updated_at_str: String = row.get(6)?;

    let created_at = parse_timestamp(&created_at_str, "created_at")?;
    let updated_at = parse_timestamp(&updated_at_str, "updated_at")?;

    Ok(Playthrough {
        id: row.get(0)?,
        name: row.get(1)?,
        difficulty: row.get(2)?,
        is_new_game_plus: row.get(3)?,
        notes: row.get(4)?,
        created_at,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::Difficulty;

    #[test]
    fn test_playthrough_crud() {
        let conn = open_in_memory().unwrap();

        let new_p = NewPlaythrough {
            name: "Death March Run 1".into(),
            difficulty: Difficulty::DeathMarch,
            is_new_game_plus: false,
            notes: Some("Planning to 100% all Gwent cards".into()),
        };

        let id = insert(&conn, &new_p).unwrap();
        assert!(id > 0);

        let p = get(&conn, id).unwrap();
        assert_eq!(p.name, "Death March Run 1");
        assert_eq!(p.difficulty, Difficulty::DeathMarch);
        assert!(!p.is_new_game_plus);
        assert_eq!(p.notes.as_deref(), Some("Planning to 100% all Gwent cards"));

        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 1);

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

        let updated_p = get(&conn, id).unwrap();
        assert_eq!(updated_p.name, "Death March NGP");
        assert!(updated_p.is_new_game_plus);

        delete(&conn, id).unwrap();
        assert!(get(&conn, id).is_err());
    }
}
