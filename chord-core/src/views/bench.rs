//! Timing tests for the view queries. Run them with
//! `cargo test -p chord-core --lib views::bench -- --ignored --nocapture`.

use std::time::Instant;

use jid::BareJid;
use rusqlite::params;

use super::{ChannelScope, QueryCtx, channel_list, diff, member_list, timeline};
use crate::store::Store;
use crate::store::queries::ensure_account;

const PEERS: usize = 50;
const PER_PEER: usize = 2000;

fn fill(store: &Store, account_id: i64) {
    let conn = store.conn();
    conn.execute_batch("BEGIN").unwrap();
    let mut stmt = conn
        .prepare(
            "INSERT INTO messages (account_id, key_kind, key, direction, peer, sender, body,
                                   timestamp, kind, stanza_id)
             VALUES (?1, 'stanza-id', ?2, ?3, ?4, ?4, 'hello there, this is a message', ?5, 'chat', ?2)",
        )
        .unwrap();
    for p in 0..PEERS {
        let peer = format!("friend{p}@chord.localhost");
        conn.execute(
            "INSERT INTO contacts (account_id, jid, name) VALUES (?1, ?2, ?3)",
            params![account_id, peer, format!("Friend {p}")],
        )
        .unwrap();
        for m in 0..PER_PEER {
            let dir = if m % 3 == 0 { "out" } else { "in" };
            stmt.execute(params![
                account_id,
                format!("{p}-{m}"),
                dir,
                peer,
                (m * 1000 + p) as i64
            ])
            .unwrap();
        }
    }
    drop(stmt);
    conn.execute_batch("COMMIT").unwrap();
}

fn time<T>(label: &str, runs: u32, mut f: impl FnMut() -> T) {
    let start = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(f());
    }
    let each = start.elapsed() / runs;
    println!("{label}: {each:?} per run");
}

#[test]
#[ignore]
fn view_query_timings() {
    let dir = std::env::temp_dir().join(format!("chord-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(dir.join("bench.db")).unwrap();
    let account = BareJid::new("alice@chord.localhost").unwrap();
    let account_id = ensure_account(store.conn(), account.as_str()).unwrap();
    let t = Instant::now();
    fill(&store, account_id);
    println!("fill {} messages: {:?}", PEERS * PER_PEER, t.elapsed());
    let q = QueryCtx {
        store: &store,
        account_id,
        account: &account,
    };
    let peer = "friend7@chord.localhost";
    time("timeline window 50", 200, || {
        timeline::query(&q, peer, 50).unwrap()
    });
    time("timeline window 500", 20, || {
        timeline::query(&q, peer, 500).unwrap()
    });
    let list = timeline::query(&q, peer, 50).unwrap();
    time("diff 50 equal items", 1000, || diff::diff(&list, &list));
    time("channel list home (50 DMs)", 20, || {
        channel_list::query(&q, &ChannelScope::Home).unwrap()
    });
    let room = BareJid::new("room@muc.chord.localhost").unwrap();
    time("unread of one peer", 200, || {
        channel_list::unread(&q, peer).unwrap()
    });
    time("member list (direct)", 200, || {
        member_list::query(&q, &room).unwrap()
    });
    let t = Instant::now();
    for i in 0..500 {
        store
            .conn()
            .execute(
                "INSERT INTO messages (account_id, key_kind, key, direction, peer, sender, body,
                                       timestamp, kind, stanza_id)
                 VALUES (?1, 'stanza-id', ?2, 'in', ?3, ?3, 'new', 9000000000, 'chat', ?2)",
                params![account_id, format!("new-{i}"), peer],
            )
            .unwrap();
    }
    println!(
        "insert one message (autocommit): {:?} each",
        t.elapsed() / 500
    );
    for (label, sql) in [
        (
            "timeline",
            "EXPLAIN QUERY PLAN SELECT id FROM messages WHERE account_id = 1 AND peer = 'x'
             AND NOT (failed_at IS NOT NULL AND retracted_at IS NOT NULL)
             ORDER BY timestamp DESC, id DESC LIMIT 50",
        ),
        (
            "home dms",
            "EXPLAIN QUERY PLAN SELECT peer, MAX(timestamp) FROM messages
             WHERE account_id = 1 AND kind = 'chat' GROUP BY peer ORDER BY MAX(timestamp) DESC",
        ),
        (
            "unread",
            "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM messages
             WHERE account_id = 1 AND peer = 'x' AND direction = 'in' AND retracted_at IS NULL
               AND id > COALESCE((SELECT last_read FROM read_state
                                  WHERE account_id = 1 AND peer = 'x'), 0)",
        ),
    ] {
        let mut stmt = store.conn().prepare(sql).unwrap();
        let rows: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        println!("plan {label}: {rows:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
