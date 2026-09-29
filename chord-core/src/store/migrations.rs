//! Schema migrations. They run at startup.
//!
//! The schema version is SQLite's `user_version`. Each migration runs in its own
//! transaction, together with the version update.

use rusqlite::Connection;

use super::StoreError;
use super::schema::MIGRATIONS;

/// Bring the database to the latest schema version. Returns the version.
pub fn migrate(conn: &mut Connection) -> Result<i64, StoreError> {
    let latest = MIGRATIONS.len() as i64;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current > latest {
        return Err(StoreError::TooNew {
            found: current,
            supported: latest,
        });
    }
    for (version, sql) in (1..).zip(MIGRATIONS).skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_twice_is_a_no_op() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(migrate(&mut conn).unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(migrate(&mut conn).unwrap(), MIGRATIONS.len() as i64);
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('accounts', 'messages')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 2);
    }

    #[test]
    fn newer_database_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1)
            .unwrap();
        assert!(migrate(&mut conn).is_err());
    }
}
