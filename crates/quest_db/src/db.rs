use std::path::Path;

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use crate::error::Result;

/// Opens (or creates) the SQLite DB at `path` and applies all pending migrations.
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    configure_pragmas(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

/// Opens an in-memory SQLite DB — useful for testing.
pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    configure_pragmas(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

fn configure_pragmas(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA synchronous = NORMAL;
        ",
    )?;
    Ok(())
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(include_str!(
        "../migrations/001_create_tables.sql"
    ))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_valid() {
        assert!(migrations().validate().is_ok());
    }

    #[test]
    fn test_open_in_memory() {
        let conn = open_in_memory().expect("failed to open in-memory db");
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("pragma query failed");
        assert_eq!(journal_mode.to_lowercase(), "memory");
    }
}
