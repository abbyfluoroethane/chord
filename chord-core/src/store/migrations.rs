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

    #[test]
    fn the_room_details_migration_keeps_rows_and_marks_shared_passwords() {
        let mut conn = Connection::open_in_memory().unwrap();
        // The version before the migration that adds `password_shared`.
        let at = MIGRATIONS
            .iter()
            .position(|sql| sql.contains("password_shared"))
            .expect("the migration exists");
        for (version, sql) in (1..).zip(&MIGRATIONS[..at]) {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        conn.execute(
            "INSERT INTO accounts (id, jid, created_at) VALUES (1, 'a@b.c', 0)",
            [],
        )
        .unwrap();
        for (jid, password, bookmarked) in [
            ("bookmarked@r.c", Some("pw"), 1),
            ("joined@r.c", Some("pw"), 0),
            ("open@r.c", None, 1),
        ] {
            conn.execute(
                "INSERT INTO rooms (account_id, jid, nick, password, bookmarked)
                 VALUES (1, ?1, 'al', ?2, ?3)",
                rusqlite::params![jid, password, bookmarked],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO occupants (account_id, room, nick) VALUES (1, 'r', 'n')",
            [],
        )
        .unwrap();

        assert_eq!(migrate(&mut conn).unwrap(), MIGRATIONS.len() as i64);

        let rows: Vec<(String, Option<String>, i64, Option<String>)> = conn
            .prepare("SELECT jid, password, password_shared, anonymity FROM rooms ORDER BY jid")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        // Nothing is lost. An old bookmark with a password had published it: it stays shared.
        assert_eq!(
            rows,
            vec![
                ("bookmarked@r.c".into(), Some("pw".into()), 1, None),
                ("joined@r.c".into(), Some("pw".into()), 0, None),
                ("open@r.c".into(), None, 0, None),
            ]
        );
        let occupant: Option<String> = conn
            .query_row("SELECT occupant_id FROM occupants", [], |r| r.get(0))
            .unwrap();
        assert_eq!(occupant, None);
    }
}
