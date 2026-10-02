//! Local storage on SQLite. One database file per account. The actor owns the only
//! connection.

pub mod json;
pub mod migrations;
pub mod queries;
pub mod schema;

use core::fmt;
use std::path::Path;

use rusqlite::Connection;

/// An error from the store.
#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    /// The database has a newer schema than this build knows.
    TooNew {
        found: i64,
        supported: i64,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "database error: {e}"),
            Self::TooNew { found, supported } => write!(
                f,
                "database schema version {found} is newer than this build (version {supported})"
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

/// The database of one account.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open or create the database at `path`, and run the migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        // WAL makes a write one sequential append, and a reader does not block it. With
        // NORMAL, a power loss can drop the last commits but never damages the file. The
        // server archive gives back lost messages.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Keep the WAL file small after a checkpoint.
        conn.pragma_update(None, "journal_size_limit", 4 << 20)?;
        Self::init(conn)
    }

    /// A database in memory. For tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "foreign_keys", true)?;
        // The code has more than 16 hot statements. A bigger cache stops the repeated
        // parsing of the evicted ones.
        conn.set_prepared_statement_cache_capacity(64);
        migrations::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
