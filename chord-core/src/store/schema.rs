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
        ask          INTEGER NOT NULL DEFAULT 0, -- bit flags: 1 = our subscription request is pending, 2 = pre-approved (roster.rs)
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
    // Version 3: message references (XEP-0308, 0424, 0444, 0461, 0333), attachments,
    // read state, and push (XEP-0357).
    r#"
    -- Every id of a message. XEPs reference the `id` attribute in a 1:1 chat and the
    -- stanza-id in a room. The key stays the stanza-id, otherwise the origin-id.
    ALTER TABLE messages ADD COLUMN message_id TEXT;
    ALTER TABLE messages ADD COLUMN origin_id TEXT;
    ALTER TABLE messages ADD COLUMN stanza_id TEXT;
    -- XEP-0461: the id that this message replies to, and the JID of its sender.
    ALTER TABLE messages ADD COLUMN reply_to TEXT;
    ALTER TABLE messages ADD COLUMN reply_to_sender TEXT;
    -- XEP-0066: an attachment URL.
    ALTER TABLE messages ADD COLUMN oob_url TEXT;
    -- XEP-0308: the newest correction.
    ALTER TABLE messages ADD COLUMN edited_body TEXT;
    ALTER TABLE messages ADD COLUMN edited_at INTEGER;
    -- XEP-0424: the time of the retraction. The body stays for the audit log only.
    ALTER TABLE messages ADD COLUMN retracted_at INTEGER;
    -- XEP-0333: for an outgoing message, the newest marker from the peer.
    ALTER TABLE messages ADD COLUMN status TEXT NOT NULL DEFAULT 'sent'
        CHECK (status IN ('sent', 'received', 'displayed'));

    UPDATE messages SET origin_id = key, message_id = key WHERE key_kind = 'origin-id';
    UPDATE messages SET stanza_id = key WHERE key_kind = 'stanza-id';
    CREATE INDEX messages_by_message_id ON messages (account_id, peer, message_id);
    CREATE INDEX messages_by_origin_id ON messages (account_id, peer, origin_id);
    CREATE INDEX messages_by_stanza_id ON messages (account_id, peer, stanza_id);

    -- XEP-0444: the reactions of one sender to one message.
    CREATE TABLE reactions (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        message    INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
        sender     TEXT NOT NULL,             -- bare JID in a 1:1 chat, occupant JID in a room
        emojis     TEXT NOT NULL,             -- JSON array of strings
        PRIMARY KEY (account_id, message, sender)
    );

    -- Our read position per chat or room: the newest message that we read (XEP-0333).
    CREATE TABLE read_state (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        peer       TEXT NOT NULL,
        last_read  INTEGER NOT NULL,          -- messages.id
        PRIMARY KEY (account_id, peer)
    );

    -- XEP-0357: the push services that this account enabled.
    CREATE TABLE push_registrations (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        service    TEXT NOT NULL,
        node       TEXT NOT NULL,
        PRIMARY KEY (account_id, service, node)
    );
    "#,
    // Version 4: pending displayed markers.
    r#"
    -- XEP-0333: the newest message (messages.id) that a displayed marker went out for.
    -- A read position that is newer than this has a marker that still waits.
    ALTER TABLE read_state ADD COLUMN marker_sent INTEGER;
    UPDATE read_state SET marker_sent = last_read;
    "#,
    // Version 5: notification levels.
    r#"
    -- The notification level of a chat, a room, or a private chat with a room occupant.
    -- A peer without a row has the default level. mute_until is Unix time in ms.
    CREATE TABLE notification_levels (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        peer       TEXT NOT NULL,             -- as messages.peer stores it
        level      TEXT NOT NULL,             -- all, mentions, or none
        mute_until INTEGER,
        PRIMARY KEY (account_id, peer)
    );
    "#,
    // 6: join requests for spaces that we own (XEP-0060, 8.6). ejabberd does not list
    // pending subscribers in the owner subscriptions query, so Chord keeps the requests
    // that the service sends.
    r#"
    CREATE TABLE space_join_requests (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        service    TEXT NOT NULL,
        node       TEXT NOT NULL,
        jid        TEXT NOT NULL,
        PRIMARY KEY (account_id, service, node, jid)
    );
    "#,
    // 7: the blocklist of XEP-0191. The server owns it. This table holds a copy for
    // offline reads and for the `blocked` flags.
    r#"
    CREATE TABLE blocked_jids (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        jid        TEXT NOT NULL,             -- as the server lists it: bare, domain, or full
        PRIMARY KEY (account_id, jid)
    );
    "#,
    // 8: our own presence: the show value and the status text. Every presence and room
    // join carries them, also after a restart.
    r#"
    CREATE TABLE own_presence (
        account_id INTEGER PRIMARY KEY REFERENCES accounts(id),
        show       TEXT,                      -- away, dnd, or xa. NULL is available.
        status     TEXT
    );
    "#,
    // 9: an outgoing chat message that the server or the peer refused (RFC 6121, 8.5): the
    // time of the error message. The status column has a CHECK list, so the failure gets
    // its own column and the timeline shows it as the status `failed`.
    r#"
    ALTER TABLE messages ADD COLUMN failed_at INTEGER;
    "#,
    // 10: edits, retractions, and reactions that arrived before their target message
    // (archive pages run newest first, and carbons can overtake the original). The row
    // waits for the target and goes when it is applied or when it gets old (orphans.rs).
    r#"
    CREATE TABLE pending_changes (
        id         INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        peer       TEXT NOT NULL,             -- as messages.peer stores it
        target     TEXT NOT NULL,             -- the id that the change names
        kind       TEXT NOT NULL CHECK (kind IN ('edit', 'retract', 'reaction')),
        msg_kind   TEXT NOT NULL,             -- 'chat' or 'groupchat'
        direction  TEXT NOT NULL,             -- 'in' or 'out'
        sender     TEXT NOT NULL,
        timestamp  INTEGER,                   -- Unix time in ms, if the stanza had one
        stanza     TEXT NOT NULL,             -- the message XML
        created_at INTEGER NOT NULL           -- Unix time in ms
    );
    CREATE INDEX pending_changes_by_target ON pending_changes (account_id, peer, target);
    "#,
    // 11: XEP-0446 file metadata of an attachment: name, size, media type, SHA-256 (base64).
    r#"
    ALTER TABLE messages ADD COLUMN file_name TEXT;
    ALTER TABLE messages ADD COLUMN file_size INTEGER;
    ALTER TABLE messages ADD COLUMN file_type TEXT;
    ALTER TABLE messages ADD COLUMN file_hash TEXT;
    "#,
    // 12: pinned messages. The server owns them, in a private PEP node (pins.rs). This table
    // holds a copy for offline reads. key is the stanza-id, origin-id, or id of the message.
    r#"
    CREATE TABLE pins (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        chat       TEXT NOT NULL,             -- bare JID of the room or the contact
        key        TEXT NOT NULL,
        sender     TEXT NOT NULL,
        body       TEXT NOT NULL,
        timestamp  INTEGER NOT NULL,          -- of the message, Unix ms
        pinned_at  INTEGER NOT NULL,          -- Unix ms
        PRIMARY KEY (account_id, chat, key)
    );
    "#,
    // 13: the publish options of a push registration (XEP-0357, for example the secret of
    // the app server), so that Chord can enable the service again at each login. The
    // idle time of a contact resource (XEP-0319). The tune of a contact (XEP-0118).
    r#"
    CREATE TABLE push_options (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        service    TEXT NOT NULL,
        node       TEXT NOT NULL,
        var        TEXT NOT NULL,
        value      TEXT NOT NULL,
        PRIMARY KEY (account_id, service, node, var)
    );
    ALTER TABLE presences ADD COLUMN idle_since TEXT;
    -- The song that a contact plays now, as "Artist - Title". The actor clears it on each
    -- new session, like the presences.
    CREATE TABLE contact_tunes (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        bare       TEXT NOT NULL,
        text       TEXT NOT NULL,
        PRIMARY KEY (account_id, bare)
    );
    "#,
    // 14: spaces. The subscription id that the service gave us (XEP-0060, 6.2.1), for the
    // unsubscribe. The subscription id and the answer state of a join request (8.6): a
    // request that we answered stays until the service sends the subscription event. The
    // nodes of a subscription list that are no space, so a start does not ask for their
    // disco#info again (spaces.rs).
    r#"
    ALTER TABLE spaces ADD COLUMN subid TEXT;
    ALTER TABLE space_join_requests ADD COLUMN subid TEXT;
    ALTER TABLE space_join_requests ADD COLUMN answered INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE non_space_nodes (
        account_id INTEGER NOT NULL REFERENCES accounts(id),
        service    TEXT NOT NULL,
        node       TEXT NOT NULL,
        PRIMARY KEY (account_id, service, node)
    );
    "#,
    // 15: room details (XEP-0045, XEP-0402, XEP-0421). The anonymity of a room from its
    // disco#info. The raw `<extensions/>` child of the room bookmark, so that a republish
    // keeps what other clients wrote. Whether the bookmark carries the room password (the
    // user agreed, or another client published it). The XEP-0421 occupant-id of each
    // occupant and of each room message. `rooms.password` is an empty string when the
    // system keychain holds the password (muc/password.rs).
    r#"
    ALTER TABLE rooms ADD COLUMN anonymity TEXT;              -- non-anonymous, semi-anonymous, anonymous
    ALTER TABLE rooms ADD COLUMN bookmark_extensions TEXT;    -- XML of the bookmark extensions
    ALTER TABLE rooms ADD COLUMN password_shared INTEGER NOT NULL DEFAULT 0;
    -- Before this version every bookmark of a room with a password carried it.
    UPDATE rooms SET password_shared = 1 WHERE bookmarked = 1 AND password IS NOT NULL;
    ALTER TABLE occupants ADD COLUMN occupant_id TEXT;
    ALTER TABLE messages ADD COLUMN occupant_id TEXT;
    "#,
    // 16: the status text of an occupant, for the member list.
    r#"
    ALTER TABLE occupants ADD COLUMN status TEXT;
    "#,
];
