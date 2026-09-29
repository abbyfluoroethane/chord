//! Table definitions. Each entry of `MIGRATIONS` is one schema version.

/// The schema, as a list of migrations. Entry `n` moves the database to version `n + 1`.
/// Never edit an entry after it ships. Add a new entry instead.
pub const MIGRATIONS: &[&str] = &[
    // Version 1: accounts and chat messages.
    r#"
    CREATE TABLE accounts (
        id         INTEGER PRIMARY KEY,
        jid        TEXT NOT NULL UNIQUE,  -- bare JID
        created_at INTEGER NOT NULL       -- Unix time in ms
    );

    CREATE TABLE messages (
        id          INTEGER PRIMARY KEY,
        account_id  INTEGER NOT NULL REFERENCES accounts(id),
        -- The key: the XEP-0359 stanza-id from our own server when it gives one,
        -- otherwise the XEP-0359 origin-id.
        key_kind    TEXT NOT NULL CHECK (key_kind IN ('stanza-id', 'origin-id')),
        key         TEXT NOT NULL,
        direction   TEXT NOT NULL CHECK (direction IN ('in', 'out')),
        peer        TEXT NOT NULL,        -- bare JID of the other side
        sender      TEXT NOT NULL,        -- JID of the sender, as in the stanza
        body        TEXT NOT NULL,
        timestamp   INTEGER NOT NULL,     -- Unix time in ms: XEP-0203 delay, or receive time
        UNIQUE (account_id, key_kind, key)
    );

    CREATE INDEX messages_by_peer ON messages (account_id, peer, timestamp);
    "#,
];
