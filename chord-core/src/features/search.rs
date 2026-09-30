//! Message search on the local store. It reads the messages that the store has: the
//! messages that the client has seen or fetched from the archive (XEP-0313), not the whole
//! archive of the server. It works offline.
//!
//! The match is a substring match with SQLite `LIKE`, so it ignores the case of ASCII
//! letters only. A corrected message matches on its new text. A retracted message does not
//! match.

use futures_channel::oneshot;
use rusqlite::params;

use super::{Ctx, FeatureCommand};
use crate::actor::{ClientError, ClientHandle};
use crate::store::Store;
use crate::store::queries::{Direction, MessageKind};

/// The most hits that one search returns.
pub const MAX_HITS: usize = 100;

type Reply = oneshot::Sender<Result<Vec<SearchHit>, ClientError>>;

/// One message that matches a search.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct SearchHit {
    /// The timeline id of the message (`m:<row id>`), the same as `TimelineItem::id`.
    pub id: String,
    pub kind: MessageKind,
    pub direction: Direction,
    /// Bare JID of the chat or room.
    pub peer: String,
    /// JID of the sender, as in the stanza.
    pub sender: String,
    /// The text, with any correction applied.
    pub body: String,
    /// Unix time in ms.
    pub timestamp: i64,
}

/// A command from the public API.
pub(crate) enum Command {
    Messages {
        peer: Option<String>,
        query: String,
        limit: usize,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Search the stored messages for `query`, newest first, at most `limit` (and never more
    /// than `MAX_HITS`). With `peer`, only the chat or room with that JID. A blank query
    /// finds nothing. Works offline.
    pub async fn search_messages(
        &self,
        peer: Option<String>,
        query: String,
        limit: usize,
    ) -> Result<Vec<SearchHit>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Search(Command::Messages {
            peer,
            query,
            limit,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    run(ctx.store, ctx.account_id, command);
}

/// Answer a command from the store. It needs no session.
pub(crate) fn run(store: &Store, account_id: i64, command: Command) {
    let Command::Messages {
        peer,
        query,
        limit,
        reply,
    } = command;
    let _ = reply.send(search(store, account_id, peer.as_deref(), &query, limit));
}

/// The text of a `LIKE` pattern that matches `query` as a substring.
fn like_pattern(query: &str) -> String {
    let mut out = String::from("%");
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

fn search(
    store: &Store,
    account_id: i64,
    peer: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, ClientError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    let limit = limit.clamp(1, MAX_HITS);
    let mut stmt = store
        .conn()
        .prepare_cached(
            "SELECT id, kind, direction, peer, sender, COALESCE(edited_body, body), timestamp
             FROM messages
             WHERE account_id = ?1 AND (?2 IS NULL OR peer = ?2)
               AND retracted_at IS NULL
               AND COALESCE(edited_body, body) LIKE ?3 ESCAPE '\\'
             ORDER BY timestamp DESC, id DESC LIMIT ?4",
        )
        .map_err(store_error)?;
    let rows = stmt
        .query_map(
            params![account_id, peer, like_pattern(query), limit as i64],
            |row| {
                Ok(SearchHit {
                    id: format!("m:{}", row.get::<_, i64>(0)?),
                    kind: MessageKind::parse(&row.get::<_, String>(1)?)?,
                    direction: Direction::parse(&row.get::<_, String>(2)?)?,
                    peer: row.get(3)?,
                    sender: row.get(4)?,
                    body: row.get(5)?,
                    timestamp: row.get(6)?,
                })
            },
        )
        .map_err(store_error)?;
    rows.collect::<rusqlite::Result<_>>().map_err(store_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::queries::{KeyKind, MessageExtras, NewMessage, insert_message};

    fn add(store: &Store, account: i64, peer: &str, key: &str, body: &str, at: i64) -> i64 {
        insert_message(
            store.conn(),
            account,
            &NewMessage {
                kind: MessageKind::Chat,
                key_kind: KeyKind::StanzaId,
                key,
                direction: Direction::In,
                peer,
                sender: peer,
                body,
                timestamp: Some(at),
                extras: MessageExtras::default(),
            },
        )
        .unwrap()
        .unwrap()
        .rowid
    }

    fn setup() -> (Store, i64) {
        let store = Store::open_in_memory().unwrap();
        let account =
            crate::store::queries::ensure_account(store.conn(), "me@example.org").unwrap();
        (store, account)
    }

    fn bodies(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.body.as_str()).collect()
    }

    #[test]
    fn finds_a_substring_newest_first_in_one_chat_or_all() {
        let (store, a) = setup();
        add(&store, a, "bob@example.org", "1", "Lunch at noon", 10);
        add(&store, a, "bob@example.org", "2", "no match", 20);
        add(&store, a, "amy@example.org", "3", "late lunch?", 30);
        let all = search(&store, a, None, "lunch", 10).unwrap();
        assert_eq!(bodies(&all), ["late lunch?", "Lunch at noon"]);
        let bob = search(&store, a, Some("bob@example.org"), "LUNCH", 10).unwrap();
        assert_eq!(bodies(&bob), ["Lunch at noon"]);
        assert_eq!(bob[0].peer, "bob@example.org");
        assert!(bob[0].id.starts_with("m:"));
    }

    #[test]
    fn blank_query_and_wildcards_are_safe() {
        let (store, a) = setup();
        add(&store, a, "bob@example.org", "1", "100% sure", 10);
        add(&store, a, "bob@example.org", "2", "a_b", 20);
        add(&store, a, "bob@example.org", "3", "axb", 30);
        assert!(search(&store, a, None, "  ", 10).unwrap().is_empty());
        assert_eq!(
            bodies(&search(&store, a, None, "%", 10).unwrap()),
            ["100% sure"]
        );
        assert_eq!(
            bodies(&search(&store, a, None, "a_b", 10).unwrap()),
            ["a_b"]
        );
    }

    #[test]
    fn respects_the_limit_edits_retractions_and_the_account() {
        let (store, a) = setup();
        let other = crate::store::queries::ensure_account(store.conn(), "you@example.org").unwrap();
        let first = add(&store, a, "bob@example.org", "1", "cat one", 10);
        add(&store, a, "bob@example.org", "2", "cat two", 20);
        add(&store, a, "bob@example.org", "3", "cat three", 30);
        add(&store, other, "bob@example.org", "4", "cat other", 40);
        assert_eq!(search(&store, a, None, "cat", 2).unwrap().len(), 2);
        // A correction matches on its new text.
        store
            .conn()
            .execute(
                "UPDATE messages SET edited_body = 'dog one' WHERE id = ?1",
                [first],
            )
            .unwrap();
        assert!(search(&store, a, None, "cat one", 10).unwrap().is_empty());
        assert_eq!(
            bodies(&search(&store, a, None, "dog", 10).unwrap()),
            ["dog one"]
        );
        // A retracted message does not match.
        store
            .conn()
            .execute(
                "UPDATE messages SET retracted_at = 50 WHERE id = ?1",
                [first],
            )
            .unwrap();
        assert!(search(&store, a, None, "dog", 10).unwrap().is_empty());
    }

    #[test]
    fn the_command_works_offline_through_the_feature_path() {
        let (store, a) = setup();
        add(&store, a, "bob@example.org", "1", "hello world", 10);
        let (reply, mut answer) = oneshot::channel();
        let changed = crate::features::on_command_offline(
            &store,
            a,
            FeatureCommand::Search(Command::Messages {
                peer: None,
                query: "world".into(),
                limit: 5,
                reply,
            }),
        );
        assert!(!changed);
        let hits = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(bodies(&hits), ["hello world"]);
    }
}
