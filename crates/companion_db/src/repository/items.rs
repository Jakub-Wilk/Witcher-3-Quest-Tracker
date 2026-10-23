use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, named_params, params};

use crate::error::Result;
use crate::models::{Item, ItemImage, NewItem};

/// Inserts or updates an item from the game files.
pub fn upsert(conn: &Connection, item: &NewItem) -> Result<()> {
    conn.execute(
        "INSERT INTO items (id, internal_name, name_key, category, quality, expansion, in_base, in_plus)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (id) DO UPDATE SET
            internal_name = excluded.internal_name, name_key = excluded.name_key,
            category = excluded.category, quality = excluded.quality,
            expansion = excluded.expansion, in_base = excluded.in_base, in_plus = excluded.in_plus,
            synced_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![
            item.id,
            item.internal_name,
            item.name_key,
            item.category,
            item.quality,
            item.expansion,
            item.in_base,
            item.in_plus,
        ],
    )?;
    Ok(())
}

/// Removes items whose id is not in `seen`, with everything that refers to them (recipes,
/// planner rows, inventory, pictures). Returns how many were removed.
pub fn delete_missing(conn: &Connection, seen: &HashSet<String>) -> Result<usize> {
    let ids: Vec<String> = conn
        .prepare("SELECT id FROM items")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut deleted = 0;
    for id in ids.iter().filter(|id| !seen.contains(*id)) {
        deleted += conn.execute("DELETE FROM items WHERE id = ?1", params![id])?;
    }
    Ok(deleted)
}

pub fn count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))?)
}

/// Every item, with names in `lang` (falling back to English, then the internal name).
pub fn list(conn: &Connection, lang: &str) -> Result<Vec<Item>> {
    let mut stmt = conn.prepare(
        "SELECT i.id, i.internal_name, COALESCE(t.text, te.text, i.internal_name),
                COALESCE(te.text, i.internal_name), i.category, i.quality, i.expansion,
                EXISTS (SELECT 1 FROM item_images im WHERE im.item_id = i.id)
         FROM items i
         LEFT JOIN game_texts t  ON t.key = i.name_key AND t.language = :lang
         LEFT JOIN game_texts te ON te.key = i.name_key AND te.language = 'en'
         ORDER BY i.id",
    )?;
    let rows = stmt.query_map(named_params! { ":lang": lang }, |row| {
        Ok(Item {
            id: row.get(0)?,
            internal_name: row.get(1)?,
            name: row.get(2)?,
            english_name: row.get(3)?,
            category: row.get(4)?,
            quality: row.get(5)?,
            expansion: row.get(6)?,
            has_image: row.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Item ids by internal name, for mapping a save's inventory.
pub fn ids_by_internal_name(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT internal_name, id FROM items ORDER BY id DESC")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    // Ordered so that a plain id wins over a `-<kind>` suffixed one with the same internal name.
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Replaces all game text in one language.
pub fn set_game_texts(conn: &Connection, language: &str, texts: &HashMap<String, String>) -> Result<()> {
    conn.execute("DELETE FROM game_texts WHERE language = ?1", params![language])?;
    let mut stmt = conn.prepare("INSERT INTO game_texts (key, language, text) VALUES (?1, ?2, ?3)")?;
    for (key, text) in texts {
        stmt.execute(params![key, language, text])?;
    }
    Ok(())
}

/// Removes the text of languages not in `keep`.
pub fn retain_game_text_languages(conn: &Connection, keep: &[String]) -> Result<()> {
    let languages: Vec<String> = conn
        .prepare("SELECT DISTINCT language FROM game_texts")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for language in languages.iter().filter(|l| !keep.contains(l)) {
        conn.execute("DELETE FROM game_texts WHERE language = ?1", params![language])?;
    }
    Ok(())
}

/// Game text for the given keys in `lang`, falling back to English.
pub fn game_texts(conn: &Connection, lang: &str, keys: &[&str]) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(
            (SELECT text FROM game_texts WHERE key = :key AND language = :lang),
            (SELECT text FROM game_texts WHERE key = :key AND language = 'en'))",
    )?;
    let mut out = HashMap::new();
    for key in keys {
        let text: Option<String> = stmt.query_row(named_params! { ":key": key, ":lang": lang }, |r| r.get(0))?;
        if let Some(text) = text {
            out.insert(key.to_string(), text);
        }
    }
    Ok(out)
}

pub fn get_image(conn: &Connection, item_id: &str) -> Result<Option<ItemImage>> {
    Ok(conn
        .query_row(
            "SELECT image, content_type, wiki_file, sha1 FROM item_images WHERE item_id = ?1",
            params![item_id],
            |r| Ok(ItemImage { image: r.get(0)?, content_type: r.get(1)?, wiki_file: r.get(2)?, sha1: r.get(3)? }),
        )
        .optional()?)
}

/// The wiki file and its SHA-1 per item with a stored picture, to skip unchanged downloads.
pub fn image_hashes(conn: &Connection) -> Result<HashMap<String, (String, String)>> {
    let mut stmt = conn.prepare("SELECT item_id, wiki_file, sha1 FROM item_images")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn set_image(conn: &Connection, item_id: &str, image: &ItemImage) -> Result<()> {
    conn.execute(
        "INSERT INTO item_images (item_id, image, content_type, wiki_file, sha1) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (item_id) DO UPDATE SET image = excluded.image, content_type = excluded.content_type,
            wiki_file = excluded.wiki_file, sha1 = excluded.sha1,
            fetched_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![item_id, image.image, image.content_type, image.wiki_file, image.sha1],
    )?;
    Ok(())
}

pub fn delete_image(conn: &Connection, item_id: &str) -> Result<()> {
    conn.execute("DELETE FROM item_images WHERE item_id = ?1", params![item_id])?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::open_in_memory;

    pub(crate) fn sample(id: &str) -> NewItem {
        NewItem {
            id: id.to_string(),
            internal_name: id.replace('-', " "),
            name_key: format!("item_name_{}", id.replace('-', "_")),
            category: "crafting_ingredient".into(),
            quality: 1,
            expansion: None,
            in_base: true,
            in_plus: true,
        }
    }

    #[test]
    fn names_fall_back_to_english_then_internal_name() {
        let conn = open_in_memory().unwrap();
        for id in ["swallow-2", "celandine", "nameless"] {
            upsert(&conn, &sample(id)).unwrap();
        }
        set_game_texts(&conn, "en", &HashMap::from([
            ("item_name_swallow_2".to_string(), "Enhanced Swallow".to_string()),
            ("item_name_celandine".to_string(), "Celandine".to_string()),
        ]))
        .unwrap();
        set_game_texts(&conn, "pl", &HashMap::from([("item_name_celandine".to_string(), "Glistnik".to_string())]))
            .unwrap();

        let items = list(&conn, "pl").unwrap();
        let name = |id: &str| items.iter().find(|i| i.id == id).unwrap().name.clone();
        assert_eq!(name("celandine"), "Glistnik");
        assert_eq!(name("swallow-2"), "Enhanced Swallow");
        assert_eq!(name("nameless"), "nameless");
        assert_eq!(items.iter().find(|i| i.id == "celandine").unwrap().english_name, "Celandine");
    }

    #[test]
    fn images_and_missing_items() {
        let conn = open_in_memory().unwrap();
        upsert(&conn, &sample("celandine")).unwrap();
        upsert(&conn, &sample("gone")).unwrap();
        let image = ItemImage {
            image: vec![1, 2, 3],
            content_type: "image/png".into(),
            wiki_file: "Celandine.png".into(),
            sha1: "abc".into(),
        };
        set_image(&conn, "gone", &image).unwrap();
        set_image(&conn, "celandine", &image).unwrap();
        assert_eq!(get_image(&conn, "celandine").unwrap(), Some(image));
        assert!(list(&conn, "en").unwrap().iter().all(|i| i.has_image));

        let deleted = delete_missing(&conn, &HashSet::from(["celandine".to_string()])).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(image_hashes(&conn).unwrap().len(), 1);
    }
}
