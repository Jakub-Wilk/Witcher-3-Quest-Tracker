use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, named_params, params};

use super::util::{QUEST_COLUMNS, QUEST_FROM, map_quest};
use crate::error::{QuestTrackerError, Result};
use crate::models::{NewQuest, Quest, QuestText};

/// Inserts or updates a quest by its journal path and replaces its texts. Returns the quest id
/// and whether it was newly inserted. Cutoff, prerequisites and sort order are left untouched.
pub fn upsert(conn: &Connection, q: &NewQuest) -> Result<(i64, bool)> {
    let existing = get_id_by_journal_path(conn, &q.journal_path)?;
    let id = match existing {
        Some(id) => {
            conn.execute(
                "UPDATE quests SET
                    journal_guid = ?2, base_name = ?3, source = ?4, quest_type = ?5, region = ?6,
                    recommended_level = ?7, wiki_page_id = ?8, wiki_title = ?9,
                    important_notes = ?10, is_unmarked = ?11,
                    synced_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                 WHERE id = ?1",
                params![
                    id,
                    q.journal_guid,
                    q.base_name,
                    q.source,
                    q.quest_type,
                    q.region,
                    q.recommended_level,
                    q.wiki_page_id,
                    q.wiki_title,
                    q.important_notes,
                    q.is_unmarked,
                ],
            )?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO quests (
                    journal_path, journal_guid, base_name, source, quest_type, region,
                    recommended_level, wiki_page_id, wiki_title, important_notes, is_unmarked
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    q.journal_path,
                    q.journal_guid,
                    q.base_name,
                    q.source,
                    q.quest_type,
                    q.region,
                    q.recommended_level,
                    q.wiki_page_id,
                    q.wiki_title,
                    q.important_notes,
                    q.is_unmarked,
                ],
            )?;
            conn.last_insert_rowid()
        }
    };
    set_texts(conn, id, &q.texts)?;
    set_extra_journal_paths(conn, id, &q.extra_journal_paths)?;
    Ok((id, existing.is_none()))
}

/// Replaces the further journal files of a quest. A path already listed for another quest
/// moves to this one.
pub fn set_extra_journal_paths(conn: &Connection, quest_id: i64, paths: &[String]) -> Result<()> {
    conn.execute("DELETE FROM quest_journals WHERE quest_id = ?1", params![quest_id])?;
    let mut stmt =
        conn.prepare("INSERT OR REPLACE INTO quest_journals (journal_path, quest_id) VALUES (?1, ?2)")?;
    for path in paths {
        stmt.execute(params![path, quest_id])?;
    }
    Ok(())
}

/// Replaces all texts of a quest.
pub fn set_texts(conn: &Connection, quest_id: i64, texts: &[QuestText]) -> Result<()> {
    conn.execute("DELETE FROM quest_texts WHERE quest_id = ?1", params![quest_id])?;
    let mut stmt = conn.prepare(
        "INSERT INTO quest_texts (quest_id, language, title, description) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for t in texts {
        stmt.execute(params![quest_id, t.language, t.title, t.description])?;
    }
    Ok(())
}

/// Retrieves a quest with its text in `language` and its prerequisite ids.
pub fn get(conn: &Connection, id: i64, language: &str) -> Result<Quest> {
    let mut quest = conn
        .query_row(
            &format!("SELECT {QUEST_COLUMNS} {QUEST_FROM} WHERE q.id = :id"),
            named_params! { ":lang": language, ":id": id },
            map_quest,
        )
        .optional()?
        .ok_or(QuestTrackerError::QuestNotFound(id))?;
    quest.prerequisite_ids = get_prerequisites(conn, id)?;
    Ok(quest)
}

pub fn get_id_by_journal_path(conn: &Connection, journal_path: &str) -> Result<Option<i64>> {
    conn.query_row("SELECT id FROM quests WHERE journal_path = ?1", params![journal_path], |row| {
        row.get(0)
    })
    .optional()
    .map_err(Into::into)
}

/// Journal path -> quest id for every journal file, including files folded into another quest
/// (those win over a leftover row of their own).
pub fn journal_ids(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut ids = HashMap::new();
    for sql in ["SELECT journal_path, id FROM quests", "SELECT journal_path, quest_id FROM quest_journals"] {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (path, id) = row?;
            ids.insert(path, id);
        }
    }
    Ok(ids)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM quests", [], |row| row.get(0))?)
}

/// Languages that have quest text, sorted.
pub fn languages(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT DISTINCT language FROM quest_texts ORDER BY language")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Every quest, in story order, with text in `language` and prerequisite ids.
pub fn list(conn: &Connection, language: &str) -> Result<Vec<Quest>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {QUEST_COLUMNS} {QUEST_FROM} ORDER BY q.sort_order ASC, q.base_name ASC"
    ))?;
    let mut quests = stmt
        .query_map(named_params! { ":lang": language }, map_quest)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut prereqs = all_prerequisites(conn)?;
    for q in &mut quests {
        q.prerequisite_ids = prereqs.remove(&q.id).unwrap_or_default();
    }
    Ok(quests)
}

/// Detaches every quest from its wiki page, so a sync starts the matching from scratch.
pub fn clear_wiki_page_ids(conn: &Connection) -> Result<()> {
    conn.execute("UPDATE quests SET wiki_page_id = NULL", [])?;
    Ok(())
}

/// Sets (or clears) the cutoff quest of a quest.
pub fn set_cutoff(conn: &Connection, id: i64, cutoff_quest_id: Option<i64>) -> Result<()> {
    let rows_affected = conn.execute(
        "UPDATE quests SET cutoff_quest_id = ?2 WHERE id = ?1",
        params![id, cutoff_quest_id],
    )?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
}

/// Sets (or clears) the display sort order of a quest.
pub fn set_sort_order(conn: &Connection, id: i64, sort_order: Option<i32>) -> Result<()> {
    let rows_affected =
        conn.execute("UPDATE quests SET sort_order = ?2 WHERE id = ?1", params![id, sort_order])?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
}

/// Replaces the full set of prerequisites for a quest.
pub fn set_prerequisites(conn: &Connection, quest_id: i64, prerequisite_ids: &[i64]) -> Result<()> {
    conn.execute("DELETE FROM quest_prerequisites WHERE quest_id = ?1", params![quest_id])?;
    for &prereq_id in prerequisite_ids {
        conn.execute(
            "INSERT OR IGNORE INTO quest_prerequisites (quest_id, prerequisite_quest_id)
             VALUES (?1, ?2)",
            params![quest_id, prereq_id],
        )?;
    }
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
    let prereqs = stmt
        .query_map(params![quest_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(prereqs)
}

/// Loads every prerequisite link in one query: quest id -> sorted prerequisite ids.
pub fn all_prerequisites(conn: &Connection) -> Result<HashMap<i64, Vec<i64>>> {
    let mut stmt = conn.prepare(
        "SELECT quest_id, prerequisite_quest_id
         FROM quest_prerequisites
         ORDER BY quest_id, prerequisite_quest_id",
    )?;
    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
    for row in rows {
        let (quest_id, prereq_id) = row?;
        map.entry(quest_id).or_default().push(prereq_id);
    }
    Ok(map)
}

/// Deletes a quest by ID, cascading to its texts, prerequisite links and progress.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let rows_affected = conn.execute("DELETE FROM quests WHERE id = ?1", params![id])?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
}

/// Outcome of [`delete_missing`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MissingQuests {
    /// Number of quests deleted.
    pub deleted: usize,
    /// Internal names of quests kept despite being missing, because a user set a status or notes.
    pub kept: Vec<String>,
}

/// Removes quests whose journal path is not in `seen` — unless a user set a status or wrote notes
/// for them in some playthrough, in which case they are kept so no user data is lost.
pub fn delete_missing(conn: &Connection, seen: &HashSet<String>) -> Result<MissingQuests> {
    let mut stmt = conn.prepare(
        "SELECT q.id, q.journal_path, q.base_name,
                EXISTS (SELECT 1 FROM quest_progress qp WHERE qp.quest_id = q.id
                        AND (qp.manual_status IS NOT NULL OR qp.notes IS NOT NULL))
         FROM quests q",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut result = MissingQuests::default();
    for (id, path, name, has_user_data) in rows {
        if seen.contains(&path) {
            continue;
        }
        if has_user_data {
            result.kept.push(name);
        } else {
            delete(conn, id)?;
            result.deleted += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::{QuestSource, QuestType, Region};

    /// A minimal quest for tests: `n` makes the journal path unique; English text only.
    pub(crate) fn sample(title: &str, n: i64) -> NewQuest {
        NewQuest {
            journal_path: format!("gameplay\\journal\\quests\\q{n:03}.journal"),
            journal_guid: format!("guid-{n}"),
            base_name: format!("Q{n:03}"),
            source: QuestSource::BaseGame,
            quest_type: QuestType::SecondaryQuest,
            region: Region::Velen,
            recommended_level: Some(5),
            wiki_page_id: None,
            wiki_title: None,
            important_notes: None,
            is_unmarked: false,
            texts: vec![QuestText { language: "en".into(), title: title.into(), description: None }],
            extra_journal_paths: vec![],
        }
    }

    #[test]
    fn extra_journal_paths_map_to_their_quest() {
        let conn = open_in_memory().unwrap();
        let (a, _) = upsert(&conn, &NewQuest { extra_journal_paths: vec!["x.journal".into()], ..sample("A", 1) }).unwrap();
        let (b, _) = upsert(&conn, &sample("B", 2)).unwrap();
        let ids = journal_ids(&conn).unwrap();
        assert_eq!(ids["x.journal"], a);
        assert_eq!(ids[&sample("", 2).journal_path], b);
        // Moving the file to another quest, then dropping it.
        upsert(&conn, &NewQuest { extra_journal_paths: vec!["x.journal".into()], ..sample("B", 2) }).unwrap();
        assert_eq!(journal_ids(&conn).unwrap()["x.journal"], b);
        upsert(&conn, &sample("B", 2)).unwrap();
        assert!(!journal_ids(&conn).unwrap().contains_key("x.journal"));
        // Deleting the quest drops its files.
        upsert(&conn, &NewQuest { extra_journal_paths: vec!["y.journal".into()], ..sample("A", 1) }).unwrap();
        delete(&conn, a).unwrap();
        assert!(!journal_ids(&conn).unwrap().contains_key("y.journal"));
    }

    #[test]
    fn upsert_inserts_then_updates_by_journal_path() {
        let conn = open_in_memory().unwrap();
        let (id, inserted) = upsert(&conn, &sample("Pyres of Novigrad", 1)).unwrap();
        assert!(inserted);
        let changed = NewQuest {
            region: Region::Novigrad,
            wiki_page_id: Some(42),
            important_notes: Some("Missable".into()),
            ..sample("Pyres of Novigrad (renamed)", 1)
        };
        let (same, inserted) = upsert(&conn, &changed).unwrap();
        assert_eq!((same, inserted), (id, false));
        let q = get(&conn, id, "en").unwrap();
        assert_eq!(q.title, "Pyres of Novigrad (renamed)");
        assert_eq!(q.region, Region::Novigrad);
        assert_eq!(q.wiki_page_id, Some(42));
        assert_eq!(count(&conn).unwrap(), 1);
    }

    #[test]
    fn text_falls_back_to_english_then_internal_name() {
        let conn = open_in_memory().unwrap();
        let mut q = sample("Kaer Morhen", 1);
        q.texts.push(QuestText {
            language: "pl".into(),
            title: "Kaer Morhen (PL)".into(),
            description: Some("Opis".into()),
        });
        let (id, _) = upsert(&conn, &q).unwrap();
        assert_eq!(get(&conn, id, "pl").unwrap().title, "Kaer Morhen (PL)");
        assert_eq!(get(&conn, id, "pl").unwrap().description.as_deref(), Some("Opis"));
        assert_eq!(get(&conn, id, "de").unwrap().title, "Kaer Morhen");
        assert_eq!(languages(&conn).unwrap(), vec!["en".to_string(), "pl".to_string()]);

        set_texts(&conn, id, &[]).unwrap();
        assert_eq!(get(&conn, id, "en").unwrap().title, "Q001");
    }

    #[test]
    fn cutoff_prerequisites_and_sort_order() {
        let conn = open_in_memory().unwrap();
        let (a, _) = upsert(&conn, &sample("A", 1)).unwrap();
        let (b, _) = upsert(&conn, &sample("B", 2)).unwrap();
        let (c, _) = upsert(&conn, &sample("C", 3)).unwrap();
        set_cutoff(&conn, a, Some(b)).unwrap();
        set_prerequisites(&conn, a, &[b, c]).unwrap();
        set_prerequisites(&conn, a, &[c]).unwrap();
        set_sort_order(&conn, a, Some(7)).unwrap();
        set_sort_order(&conn, b, Some(1)).unwrap();
        set_sort_order(&conn, c, Some(2)).unwrap();

        let q = get(&conn, a, "en").unwrap();
        assert_eq!(q.cutoff_quest_id, Some(b));
        assert_eq!(q.prerequisite_ids, vec![c]);
        let order: Vec<i64> = list(&conn, "en").unwrap().iter().map(|q| q.id).collect();
        assert_eq!(order, vec![b, c, a]);
        assert_eq!(journal_ids(&conn).unwrap().len(), 3);

        // Deleting the cutoff target clears the reference.
        delete(&conn, b).unwrap();
        assert_eq!(get(&conn, a, "en").unwrap().cutoff_quest_id, None);
        assert!(matches!(set_cutoff(&conn, 999, None), Err(QuestTrackerError::QuestNotFound(999))));
    }

    #[test]
    fn delete_missing_keeps_quests_with_user_data() {
        use crate::models::{Difficulty, NewPlaythrough, QuestStatus};
        use crate::repository::{playthroughs, progress};

        let conn = open_in_memory().unwrap();
        let (kept, _) = upsert(&conn, &sample("Has a manual status", 1)).unwrap();
        let (gone, _) = upsert(&conn, &sample("Only save status", 2)).unwrap();
        let (seen, _) = upsert(&conn, &sample("Still in the game", 3)).unwrap();
        let pt = playthroughs::insert(
            &conn,
            &NewPlaythrough {
                name: "Run".into(),
                difficulty: Difficulty::DeathMarch,
                is_new_game_plus: false,
                notes: None,
                link: None,
            },
        )
        .unwrap();
        progress::set_manual_status(&conn, pt, kept, Some(QuestStatus::Completed)).unwrap();
        progress::apply_save_statuses(&conn, pt, &HashMap::from([(gone, QuestStatus::Completed)]))
            .unwrap();

        let result = delete_missing(&conn, &HashSet::from([sample("", 3).journal_path])).unwrap();
        assert_eq!(result, MissingQuests { deleted: 1, kept: vec!["Q001".into()] });
        assert!(get(&conn, kept, "en").is_ok());
        assert!(get(&conn, gone, "en").is_err());
        assert!(get(&conn, seen, "en").is_ok());
    }
}
