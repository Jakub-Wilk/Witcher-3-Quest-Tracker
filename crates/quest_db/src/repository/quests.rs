use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use super::util::{QUEST_COLUMNS, map_quest};
use crate::error::{QuestTrackerError, Result};
use crate::models::{NewQuest, Quest, QuestFilter};

/// Inserts static quest reference data into the database and returns the generated ID.
/// Also inserts any specified prerequisite relationships.
pub fn insert(conn: &Connection, q: &NewQuest) -> Result<i64> {
    conn.execute(
        "INSERT INTO quests (
            wiki_page_id, wiki_title, name, localized_name, source, quest_type, region,
            recommended_level, sort_order, description, important_notes, is_unmarked,
            cutoff_quest_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            q.wiki_page_id,
            q.wiki_title,
            q.name,
            q.localized_name,
            q.source,
            q.quest_type,
            q.region,
            q.recommended_level,
            q.sort_order,
            q.description,
            q.important_notes,
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

/// Updates the reference data of an existing quest from `q`.
///
/// Cutoff, prerequisites and sort order are managed separately (`set_cutoff`,
/// `set_prerequisites`, `set_sort_order`); per-playthrough progress is never affected.
pub fn update(conn: &Connection, id: i64, q: &NewQuest) -> Result<()> {
    let rows_affected = conn.execute(
        "UPDATE quests SET
            wiki_page_id = ?2, wiki_title = ?3, name = ?4, localized_name = ?5, source = ?6,
            quest_type = ?7, region = ?8, recommended_level = ?9, description = ?10,
            important_notes = ?11, is_unmarked = ?12
         WHERE id = ?1",
        params![
            id,
            q.wiki_page_id,
            q.wiki_title,
            q.name,
            q.localized_name,
            q.source,
            q.quest_type,
            q.region,
            q.recommended_level,
            q.description,
            q.important_notes,
            q.is_unmarked,
        ],
    )?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
}

/// Retrieves a quest by its ID along with its list of prerequisite quest IDs.
pub fn get(conn: &Connection, id: i64) -> Result<Quest> {
    let mut quest = conn
        .query_row(
            &format!("SELECT {QUEST_COLUMNS} FROM quests q WHERE q.id = ?1"),
            params![id],
            map_quest,
        )
        .optional()?
        .ok_or(QuestTrackerError::QuestNotFound(id))?;

    quest.prerequisite_ids = get_prerequisites(conn, id)?;
    Ok(quest)
}

/// Looks up a quest's ID by its MediaWiki page id.
pub fn get_id_by_page_id(conn: &Connection, wiki_page_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM quests WHERE wiki_page_id = ?1",
        params![wiki_page_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

/// Queries quests with optional filter criteria and populates prerequisite IDs for each quest.
pub fn list(conn: &Connection, filter: &QuestFilter) -> Result<Vec<Quest>> {
    let mut query = format!("SELECT {QUEST_COLUMNS} FROM quests q WHERE 1=1");
    let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(source) = filter.source {
        param_values.push(Box::new(source));
        query.push_str(&format!(" AND q.source = ?{}", param_values.len()));
    }
    if let Some(quest_type) = filter.quest_type {
        param_values.push(Box::new(quest_type));
        query.push_str(&format!(" AND q.quest_type = ?{}", param_values.len()));
    }
    if let Some(region) = filter.region {
        param_values.push(Box::new(region));
        query.push_str(&format!(" AND q.region = ?{}", param_values.len()));
    }
    if let Some(is_unmarked) = filter.is_unmarked {
        param_values.push(Box::new(is_unmarked));
        query.push_str(&format!(" AND q.is_unmarked = ?{}", param_values.len()));
    }
    if let Some(max_lvl) = filter.max_recommended_level {
        param_values.push(Box::new(max_lvl));
        query.push_str(&format!(
            " AND (q.recommended_level IS NULL OR q.recommended_level <= ?{})",
            param_values.len()
        ));
    }
    query.push_str(" ORDER BY q.sort_order ASC, q.name ASC");

    let mut stmt = conn.prepare(&query)?;
    let params_slice: Vec<&dyn rusqlite::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    let mut quests = stmt
        .query_map(params_slice.as_slice(), map_quest)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut prereqs = all_prerequisites(conn)?;
    for q in &mut quests {
        q.prerequisite_ids = prereqs.remove(&q.id).unwrap_or_default();
    }
    Ok(quests)
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
    let rows_affected = conn.execute(
        "UPDATE quests SET sort_order = ?2 WHERE id = ?1",
        params![id, sort_order],
    )?;
    if rows_affected == 0 {
        return Err(QuestTrackerError::QuestNotFound(id));
    }
    Ok(())
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

/// Replaces the full set of prerequisites for a quest.
pub fn set_prerequisites(conn: &Connection, quest_id: i64, prerequisite_ids: &[i64]) -> Result<()> {
    conn.execute(
        "DELETE FROM quest_prerequisites WHERE quest_id = ?1",
        params![quest_id],
    )?;
    for &prereq_id in prerequisite_ids {
        add_prerequisite(conn, quest_id, prereq_id)?;
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

/// Deletes a quest by ID. Deleting a quest cascades to deleting its prerequisite links.
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
    /// Names of quests kept despite being missing, because some playthrough has progress on them.
    pub kept: Vec<String>,
}

/// Removes quests whose wiki page id is not in `seen_page_ids` — unless any playthrough has
/// progress recorded for them, in which case they are kept so no user data is lost.
pub fn delete_missing(conn: &Connection, seen_page_ids: &HashSet<i64>) -> Result<MissingQuests> {
    let mut stmt = conn.prepare(
        "SELECT q.id, q.wiki_page_id, q.name,
                EXISTS (SELECT 1 FROM quest_progress qp WHERE qp.quest_id = q.id)
         FROM quests q",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut result = MissingQuests::default();
    for (id, page_id, name, has_progress) in rows {
        if seen_page_ids.contains(&page_id) {
            continue;
        }
        if has_progress {
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

    /// A minimal quest for tests; `page_id` doubles as a unique suffix for the title.
    pub(crate) fn sample(name: &str, page_id: i64) -> NewQuest {
        NewQuest {
            wiki_page_id: page_id,
            wiki_title: name.into(),
            name: name.into(),
            localized_name: None,
            source: QuestSource::BaseGame,
            quest_type: QuestType::SecondaryQuest,
            region: Region::Velen,
            recommended_level: Some(5),
            sort_order: None,
            description: None,
            important_notes: None,
            is_unmarked: false,
            cutoff_quest_id: None,
            prerequisite_ids: vec![],
        }
    }

    #[test]
    fn test_quest_crud_cutoff_and_prerequisites() {
        let conn = open_in_memory().unwrap();

        let q1_id = insert(&conn, &sample("Pyres of Novigrad", 1)).unwrap();
        let q2_id = insert(
            &conn,
            &NewQuest { prerequisite_ids: vec![q1_id], ..sample("The Isle of Mists", 2) },
        )
        .unwrap();
        let q3_id = insert(
            &conn,
            &NewQuest {
                region: Region::Novigrad,
                is_unmarked: true,
                cutoff_quest_id: Some(q2_id),
                prerequisite_ids: vec![q1_id],
                ..sample("Witch Hunter Raids", 3)
            },
        )
        .unwrap();

        let q3 = get(&conn, q3_id).unwrap();
        assert_eq!(q3.name, "Witch Hunter Raids");
        assert_eq!(q3.wiki_page_id, 3);
        assert_eq!(q3.cutoff_quest_id, Some(q2_id));
        assert_eq!(q3.prerequisite_ids, vec![q1_id]);
        assert!(q3.is_unmarked);

        assert_eq!(get_id_by_page_id(&conn, 3).unwrap(), Some(q3_id));
        assert_eq!(get_id_by_page_id(&conn, 99).unwrap(), None);

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
        assert_eq!(filtered[0].prerequisite_ids, vec![q1_id]);
    }

    #[test]
    fn test_update_cutoff_prerequisites_and_sort_order() {
        let conn = open_in_memory().unwrap();
        let a = insert(&conn, &sample("A", 1)).unwrap();
        let b = insert(&conn, &sample("B", 2)).unwrap();
        let c = insert(&conn, &sample("C", 3)).unwrap();

        let changed = NewQuest {
            region: Region::Skellige,
            recommended_level: Some(20),
            description: Some("Updated".into()),
            localized_name: Some("Ä".into()),
            important_notes: Some("Missable".into()),
            ..sample("A renamed", 1)
        };
        update(&conn, a, &changed).unwrap();
        set_cutoff(&conn, a, Some(b)).unwrap();
        set_prerequisites(&conn, a, &[b, c]).unwrap();
        set_prerequisites(&conn, a, &[c]).unwrap();
        set_sort_order(&conn, a, Some(7)).unwrap();

        let q = get(&conn, a).unwrap();
        assert_eq!(q.name, "A renamed");
        assert_eq!(q.display_name(), "Ä");
        assert_eq!(q.region, Region::Skellige);
        assert_eq!(q.recommended_level, Some(20));
        assert_eq!(q.description.as_deref(), Some("Updated"));
        assert_eq!(q.important_notes.as_deref(), Some("Missable"));
        assert_eq!(q.cutoff_quest_id, Some(b));
        assert_eq!(q.prerequisite_ids, vec![c]);
        assert_eq!(q.sort_order, Some(7));

        set_cutoff(&conn, a, None).unwrap();
        assert_eq!(get(&conn, a).unwrap().cutoff_quest_id, None);

        assert!(matches!(update(&conn, 999, &changed), Err(QuestTrackerError::QuestNotFound(999))));
    }

    #[test]
    fn test_delete_missing_keeps_quests_with_progress() {
        use crate::models::{Difficulty, NewPlaythrough, QuestProgressUpdate, QuestStatus};
        use crate::repository::{playthroughs, progress};

        let conn = open_in_memory().unwrap();
        let kept = insert(&conn, &sample("Has progress", 1)).unwrap();
        let gone = insert(&conn, &sample("No progress", 2)).unwrap();
        let seen = insert(&conn, &sample("Still on wiki", 3)).unwrap();

        let pt = playthroughs::insert(
            &conn,
            &NewPlaythrough {
                name: "Run".into(),
                difficulty: Difficulty::DeathMarch,
                is_new_game_plus: false,
                notes: None,
            },
        )
        .unwrap();
        progress::update_status(
            &conn,
            pt,
            kept,
            &QuestProgressUpdate { status: Some(QuestStatus::Completed), ..Default::default() },
        )
        .unwrap();

        let result = delete_missing(&conn, &HashSet::from([3])).unwrap();
        assert_eq!(result, MissingQuests { deleted: 1, kept: vec!["Has progress".into()] });
        assert!(get(&conn, kept).is_ok());
        assert!(get(&conn, gone).is_err());
        assert!(get(&conn, seen).is_ok());
    }

    #[test]
    fn test_wiki_page_id_is_unique() {
        let conn = open_in_memory().unwrap();
        insert(&conn, &sample("A", 1)).unwrap();
        assert!(insert(&conn, &sample("B", 1)).is_err());
    }
}
