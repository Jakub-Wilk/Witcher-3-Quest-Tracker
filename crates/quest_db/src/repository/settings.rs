//! Key/value store for small UI preferences (last opened playthrough, sync language, ...).

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;

/// Returns the value stored under `key`, if any.
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM app_settings WHERE key = ?1", params![key], |row| row.get(0))
        .optional()
        .map_err(Into::into)
}

/// Stores `value` under `key`, replacing any previous value.
pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = EXCLUDED.value",
        params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn test_get_set_overwrite() {
        let conn = open_in_memory().unwrap();
        assert_eq!(get(&conn, "lang").unwrap(), None);
        set(&conn, "lang", "pl").unwrap();
        set(&conn, "lang", "ru").unwrap();
        assert_eq!(get(&conn, "lang").unwrap().as_deref(), Some("ru"));
    }
}
