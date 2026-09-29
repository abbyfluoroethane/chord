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
    // Version 2: the tables for the MVP features.
    r#"
    -- 'chat' for 1:1 messages, 'groupchat' for MUC messages. For a groupchat message,
    -- peer is the room JID and sender is room@service/nick.
    ALTER TABLE messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'
        CHECK (kind IN ('chat', 'groupchat'));

    -- Roster (RFC 6121).
    CREATE TABLE contacts (
        account_id   INTEGER NOT NULL REFERENCES accounts(id),
        jid          TEXT NOT NULL,              -- bare JID
        name         TEXT,
        subscription TEXT NOT NULL DEFAULT 'none',
        ask          INTEGER NOT NULL DEFAULT 0, -- 1 if a subscription request is pending
        groups       TEXT NOT NULL DEFAULT '[]', -- JSON array of group names
        PRIMARY KEY (account_id, jid)
    );
    CREATE TABLE roster_versions (
        account_id INTEGER PRIMARY KEY REFERENCES accounts(id),
        version    TEXT NOT NULL
    );

    -- Presence of contacts, per resource. The actor clears it on each new session.
    CREATE TABLE presences (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        jid        TEXT NOT NULL,                -- full JID
        bare       TEXT NOT NULL,
        show       TEXT,                         -- NULL = available, or away/chat/dnd/xa
        status     TEXT,
        priority   INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (account_id, jid)
    );

    -- MUC rooms that the account knows: from bookmarks (XEP-0402) or from a join.
    CREATE TABLE rooms (
        account_id  INTEGER NOT NULL REFERENCES accounts(id),
        jid         TEXT NOT NULL,               -- room bare JID
        name        TEXT,
        nick        TEXT,
        password    TEXT,
        autojoin    INTEGER NOT NULL DEFAULT 0,
        bookmarked  INTEGER NOT NULL DEFAULT 0,
        joined      INTEGER NOT NULL DEFAULT 0,  -- volatile: cleared on each new session
        subject     TEXT,
        PRIMARY KEY (account_id, jid)
    );

    -- Room occupants. Volatile: the actor clears it on each new session.
    CREATE TABLE occupants (
        account_id  INTEGER NOT NULL REFERENCES accounts(id),
        room        TEXT NOT NULL,
        nick        TEXT NOT NULL,
        real_jid    TEXT,                        -- if the room shows it
        affiliation TEXT NOT NULL DEFAULT 'none',
        role        TEXT NOT NULL DEFAULT 'participant',
        show        TEXT,
        PRIMARY KEY (account_id, room, nick)
    );

    -- XEP-0503 spaces that the account follows.
    CREATE TABLE spaces (
        account_id   INTEGER NOT NULL REFERENCES accounts(id),
        service      TEXT NOT NULL,              -- pubsub service JID
        node         TEXT NOT NULL,
        name         TEXT,
        description  TEXT,
        access_model TEXT,
        subscribed   INTEGER NOT NULL DEFAULT 0,
        position     INTEGER NOT NULL DEFAULT 0, -- order in the space rail
        PRIMARY KEY (account_id, service, node)
    );

    -- The items of a space node. A room item has room_jid. Other items keep their XML.
    CREATE TABLE space_items (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        service    TEXT NOT NULL,
        node       TEXT NOT NULL,
        item_id    TEXT NOT NULL,
        room_jid   TEXT,
        name       TEXT,
        category   TEXT,
        position   INTEGER NOT NULL DEFAULT 0,
        payload    TEXT,                         -- raw XML of the item payload
        PRIMARY KEY (account_id, service, node, item_id)
    );

    -- MAM sync state per archive: '' is the account archive, else a room JID.
    CREATE TABLE sync_cursors (
        account_id      INTEGER NOT NULL REFERENCES accounts(id),
        archive         TEXT NOT NULL,
        newest_id       TEXT,                    -- newest archive id that is stored
        oldest_id       TEXT,                    -- oldest archive id that is stored
        history_complete INTEGER NOT NULL DEFAULT 0, -- 1 when the oldest page arrived
        PRIMARY KEY (account_id, archive)
    );

    -- XEP-0084 avatars of contacts, rooms, and spaces.
    CREATE TABLE avatars (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        owner      TEXT NOT NULL,                -- JID, or service/node for a space
        hash       TEXT NOT NULL,                -- SHA-1 of the image (the XEP-0084 id)
        mime       TEXT,
        data       BLOB,                         -- NULL until the data arrives
        PRIMARY KEY (account_id, owner)
    );
    "#,
];
