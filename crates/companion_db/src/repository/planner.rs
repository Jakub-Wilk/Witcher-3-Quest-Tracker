//! Recipe planner state and the inventory, per playthrough.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};

use crate::error::Result;
use crate::models::QueueEntry;

/// The planned crafts, in the order they were added.
pub fn queue(conn: &Connection, playthrough_id: i64) -> Result<Vec<QueueEntry>> {
    let mut stmt = conn.prepare(
        "SELECT item_id, qty, variant FROM planner_queue WHERE playthrough_id = ?1 ORDER BY position, item_id",
    )?;
    let rows = stmt.query_map(params![playthrough_id], |r| {
        Ok(QueueEntry { item_id: r.get(0)?, qty: r.get(1)?, variant: r.get::<_, u32>(2)? as usize })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Adds `qty` crafts of a product, appending it to the queue if it is not planned yet.
pub fn add(conn: &Connection, playthrough_id: i64, item_id: &str, qty: u32) -> Result<()> {
    conn.execute(
        "INSERT INTO planner_queue (playthrough_id, item_id, qty, position)
         VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), -1) + 1 FROM planner_queue WHERE playthrough_id = ?1))
         ON CONFLICT (playthrough_id, item_id) DO UPDATE SET qty = qty + excluded.qty",
        params![playthrough_id, item_id, qty],
    )?;
    Ok(())
}

/// Sets how many crafts of a product are planned; 0 removes it from the queue.
pub fn set_qty(conn: &Connection, playthrough_id: i64, item_id: &str, qty: u32) -> Result<()> {
    if qty == 0 {
        conn.execute(
            "DELETE FROM planner_queue WHERE playthrough_id = ?1 AND item_id = ?2",
            params![playthrough_id, item_id],
        )?;
    } else {
        conn.execute(
            "UPDATE planner_queue SET qty = ?3 WHERE playthrough_id = ?1 AND item_id = ?2",
            params![playthrough_id, item_id, qty],
        )?;
    }
    Ok(())
}

pub fn set_variant(conn: &Connection, playthrough_id: i64, item_id: &str, variant: usize) -> Result<()> {
    conn.execute(
        "UPDATE planner_queue SET variant = ?3 WHERE playthrough_id = ?1 AND item_id = ?2",
        params![playthrough_id, item_id, variant as i64],
    )?;
    Ok(())
}

/// Empties the queue and forgets collected marks; craft-it-myself choices are kept.
pub fn clear(conn: &Connection, playthrough_id: i64) -> Result<()> {
    conn.execute("DELETE FROM planner_queue WHERE playthrough_id = ?1", params![playthrough_id])?;
    conn.execute("DELETE FROM planner_collected WHERE playthrough_id = ?1", params![playthrough_id])?;
    Ok(())
}

fn id_set(conn: &Connection, table: &str, playthrough_id: i64) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(&format!("SELECT item_id FROM {table} WHERE playthrough_id = ?1"))?;
    let rows = stmt.query_map(params![playthrough_id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn set_flag(conn: &Connection, table: &str, playthrough_id: i64, item_id: &str, on: bool) -> Result<()> {
    let sql = if on {
        format!("INSERT OR IGNORE INTO {table} (playthrough_id, item_id) VALUES (?1, ?2)")
    } else {
        format!("DELETE FROM {table} WHERE playthrough_id = ?1 AND item_id = ?2")
    };
    conn.execute(&sql, params![playthrough_id, item_id])?;
    Ok(())
}

/// Ingredients marked as gathered by hand.
pub fn collected(conn: &Connection, playthrough_id: i64) -> Result<HashSet<String>> {
    id_set(conn, "planner_collected", playthrough_id)
}

pub fn set_collected(conn: &Connection, playthrough_id: i64, item_id: &str, on: bool) -> Result<()> {
    set_flag(conn, "planner_collected", playthrough_id, item_id, on)
}

/// Craftable ingredients the user will craft instead of gathering.
pub fn craft_self(conn: &Connection, playthrough_id: i64) -> Result<HashSet<String>> {
    id_set(conn, "planner_craft_self", playthrough_id)
}

pub fn set_craft_self(conn: &Connection, playthrough_id: i64, item_id: &str, on: bool) -> Result<()> {
    set_flag(conn, "planner_craft_self", playthrough_id, item_id, on)
}

/// Item id → quantity carried, as of the last imported save.
pub fn inventory(conn: &Connection, playthrough_id: i64) -> Result<HashMap<String, u32>> {
    let mut stmt = conn.prepare("SELECT item_id, qty FROM inventory WHERE playthrough_id = ?1")?;
    let rows = stmt.query_map(params![playthrough_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Replaces a playthrough's inventory with the one read from the save made at `save_key`.
pub fn replace_inventory(
    conn: &Connection,
    playthrough_id: i64,
    save_key: i64,
    items: &HashMap<String, u32>,
) -> Result<()> {
    conn.execute("DELETE FROM inventory WHERE playthrough_id = ?1", params![playthrough_id])?;
    let mut stmt = conn.prepare("INSERT INTO inventory (playthrough_id, item_id, qty) VALUES (?1, ?2, ?3)")?;
    for (item_id, qty) in items {
        stmt.execute(params![playthrough_id, item_id, qty])?;
    }
    conn.execute(
        "UPDATE playthroughs SET inventory_save_key = ?2 WHERE id = ?1",
        params![playthrough_id, save_key],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::NewPlaythrough;
    use crate::repository::{items, playthroughs};

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        for id in ["swallow-2", "celandine", "hardened-leather"] {
            items::upsert(&conn, &items::tests::sample(id)).unwrap();
        }
        let pt = playthroughs::insert(
            &conn,
            &NewPlaythrough { name: "Run".into(), is_new_game_plus: false, notes: None, link: None },
        )
        .unwrap();
        (conn, pt)
    }

    #[test]
    fn queue_adds_up_and_keeps_order() {
        let (conn, pt) = setup();
        add(&conn, pt, "swallow-2", 1).unwrap();
        add(&conn, pt, "hardened-leather", 2).unwrap();
        add(&conn, pt, "swallow-2", 2).unwrap();
        set_variant(&conn, pt, "swallow-2", 1).unwrap();
        let q = queue(&conn, pt).unwrap();
        assert_eq!(q.iter().map(|e| (e.item_id.as_str(), e.qty, e.variant)).collect::<Vec<_>>(), vec![
            ("swallow-2", 3, 1),
            ("hardened-leather", 2, 0)
        ]);
        set_qty(&conn, pt, "swallow-2", 0).unwrap();
        assert_eq!(queue(&conn, pt).unwrap().len(), 1);
    }

    #[test]
    fn flags_and_inventory() {
        let (conn, pt) = setup();
        set_collected(&conn, pt, "celandine", true).unwrap();
        set_collected(&conn, pt, "celandine", true).unwrap();
        set_craft_self(&conn, pt, "hardened-leather", true).unwrap();
        assert_eq!(collected(&conn, pt).unwrap(), HashSet::from(["celandine".to_string()]));
        assert_eq!(craft_self(&conn, pt).unwrap().len(), 1);

        replace_inventory(&conn, pt, 42, &HashMap::from([("celandine".to_string(), 7)])).unwrap();
        replace_inventory(&conn, pt, 43, &HashMap::from([("swallow-2".to_string(), 1)])).unwrap();
        assert_eq!(inventory(&conn, pt).unwrap(), HashMap::from([("swallow-2".to_string(), 1)]));
        assert_eq!(playthroughs::get(&conn, pt).unwrap().inventory_save_key, Some(43));

        add(&conn, pt, "swallow-2", 1).unwrap();
        clear(&conn, pt).unwrap();
        assert!(queue(&conn, pt).unwrap().is_empty());
        assert!(collected(&conn, pt).unwrap().is_empty());
        assert_eq!(craft_self(&conn, pt).unwrap().len(), 1);

        // Deleting the playthrough takes its planner state along.
        playthroughs::delete(&conn, pt).unwrap();
        let rows: i64 = conn.query_row("SELECT COUNT(*) FROM planner_craft_self", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0);
    }
}
