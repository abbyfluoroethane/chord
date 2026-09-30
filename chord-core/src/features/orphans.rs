//! Changes that arrive before their target (XEP-0308, XEP-0424, XEP-0444).
//!
//! An archive page runs newest first, and a carbon can overtake the original. So an edit, a
//! retraction, or a reaction can arrive when the message that it names is not in the store.
//! The extension features call `stash` for such a change. It keeps the stanza in
//! `pending_changes`, keyed by the id that it names. `after_store` runs when a new message
//! is stored: it runs the waiting changes against it, and deletes them.
//!
//! A change waits for 7 days. Each account keeps at most `MAX_PENDING` of them, and the
//! oldest go first.

use rusqlite::params;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;

use super::message_ext::Incoming;
use super::{Ctx, corrections, reactions, retraction};
use crate::store::queries::{Direction, MessageKind, StoredMessage};

/// How long a change waits for its target, in ms.
const MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// The most changes that one account keeps.
const MAX_PENDING: i64 = 500;

/// What a waiting change does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Change {
    Edit,
    Retract,
    Reaction,
}

impl Change {
    fn as_str(self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Retract => "retract",
            Self::Reaction => "reaction",
        }
    }
}

/// Keep a change for the message `target` of `incoming.peer`, which is not stored yet.
pub(crate) fn stash(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>, target: &str, change: Change) {
    let mut xml = Vec::new();
    if Element::from(incoming.message.clone())
        .write_to(&mut xml)
        .is_err()
    {
        return;
    }
    let stanza = String::from_utf8_lossy(&xml).into_owned();
    let conn = ctx.store.conn();
    let now = now_ms(ctx);
    let result = conn
        .execute(
            "DELETE FROM pending_changes WHERE account_id = ?1 AND created_at < ?2",
            params![ctx.account_id, now - MAX_AGE_MS],
        )
        .and_then(|_| {
            conn.execute(
                "INSERT INTO pending_changes
                   (account_id, peer, target, kind, msg_kind, direction, sender, timestamp,
                    stanza, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    ctx.account_id,
                    incoming.peer,
                    target,
                    change.as_str(),
                    incoming.kind.as_str(),
                    incoming.direction.as_str(),
                    incoming.sender,
                    incoming.timestamp,
                    stanza,
                    now,
                ],
            )
        })
        .and_then(|_| {
            conn.execute(
                "DELETE FROM pending_changes WHERE account_id = ?1 AND id NOT IN
                   (SELECT id FROM pending_changes WHERE account_id = ?1
                    ORDER BY id DESC LIMIT ?2)",
                params![ctx.account_id, MAX_PENDING],
            )
        });
    match result {
        Ok(_) => log::debug!(
            "{} for {target} in {} waits for its target",
            change.as_str(),
            incoming.peer
        ),
        Err(e) => ctx.store_error("keep a change for a missing message", e),
    }
}

fn now_ms(ctx: &Ctx<'_>) -> i64 {
    ctx.store
        .conn()
        .query_row(&format!("SELECT {}", corrections::NOW_MS), [], |r| r.get(0))
        .unwrap_or(0)
}

/// A waiting change, as the table holds it.
struct Pending {
    id: i64,
    kind: String,
    msg_kind: String,
    direction: String,
    sender: String,
    timestamp: Option<i64>,
    stanza: String,
}

/// A new message is in the store: apply the changes that wait for it.
pub(crate) fn after_store(ctx: &mut Ctx<'_>, incoming: &Incoming<'_>, stored: &StoredMessage) {
    let waiting = match waiting_for(ctx, incoming.peer, stored.rowid) {
        Ok(w) => w,
        Err(e) => {
            ctx.store_error("read the changes that wait", e);
            return;
        }
    };
    for p in waiting {
        if let Err(e) = ctx
            .store
            .conn()
            .execute("DELETE FROM pending_changes WHERE id = ?1", [p.id])
        {
            ctx.store_error("delete a waiting change", e);
            continue;
        }
        let (Ok(element), Ok(msg_kind), Ok(direction)) = (
            p.stanza.parse::<Element>(),
            MessageKind::parse(&p.msg_kind),
            Direction::parse(&p.direction),
        ) else {
            continue;
        };
        let Ok(message) = Message::try_from(element) else {
            continue;
        };
        let replay = Incoming {
            message: &message,
            kind: msg_kind,
            direction,
            peer: incoming.peer,
            sender: &p.sender,
            timestamp: p.timestamp,
        };
        log::debug!(
            "applying a {} that waited for message {}",
            p.kind,
            stored.rowid
        );
        match p.kind.as_str() {
            "edit" => corrections::on_message(ctx, &replay),
            "retract" => retraction::on_message(ctx, &replay),
            "reaction" => reactions::on_message(ctx, &replay),
            _ => false,
        };
    }
}

/// The changes that name an id of the message `rowid`, oldest first.
fn waiting_for(ctx: &Ctx<'_>, peer: &str, rowid: i64) -> rusqlite::Result<Vec<Pending>> {
    let mut stmt = ctx.store.conn().prepare_cached(
        "SELECT p.id, p.kind, p.msg_kind, p.direction, p.sender, p.timestamp, p.stanza
         FROM pending_changes p JOIN messages m
           ON m.account_id = p.account_id AND m.peer = p.peer
          AND p.target IN (m.message_id, m.origin_id, m.stanza_id)
         WHERE p.account_id = ?1 AND p.peer = ?2 AND m.id = ?3
         ORDER BY p.id",
    )?;
    stmt.query_map(params![ctx.account_id, peer, rowid], |row| {
        Ok(Pending {
            id: row.get(0)?,
            kind: row.get(1)?,
            msg_kind: row.get(2)?,
            direction: row.get(3)?,
            sender: row.get(4)?,
            timestamp: row.get(5)?,
            stanza: row.get(6)?,
        })
    })?
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::corrections::tests::store_row;
    use crate::features::testing::Harness;
    use crate::store::queries::{self, KeyKind, StoredMessage};
    use xmpp_parsers::message::Id;
    use xmpp_parsers::message_correct::Replace;

    use xmpp_parsers::minidom::rxml::NcName;

    fn nc(name: &str) -> NcName {
        NcName::try_from(name.to_owned()).unwrap()
    }

    const BOB: &str = "bob@chord.localhost";
    const FROM: &str = "bob@chord.localhost/laptop";

    /// Deliver a change to the intercept hook. Returns true if the message is handled.
    fn deliver(h: &mut Harness, m: &Message, timestamp: Option<i64>) -> bool {
        let incoming = Incoming {
            message: m,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: BOB,
            sender: FROM,
            timestamp,
        };
        h.with_ctx(|ctx| super::super::message_ext::intercept(ctx, &incoming))
    }

    /// The target arrives: store it and run `after_store`.
    fn target_arrives(h: &mut Harness, id: &str) -> String {
        let item = store_row(h, MessageKind::Chat, Direction::In, BOB, FROM, id, None);
        let row = queries::find_by_timeline_id(h.store.conn(), h.account_id, &item)
            .unwrap()
            .unwrap();
        let stored = StoredMessage {
            rowid: row.rowid,
            kind: MessageKind::Chat,
            key_kind: KeyKind::OriginId,
            key: id.into(),
            direction: Direction::In,
            peer: BOB.into(),
            sender: FROM.into(),
            body: "old".into(),
            timestamp: 0,
        };
        let m = Message::chat(None);
        let incoming = Incoming {
            message: &m,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: BOB,
            sender: FROM,
            timestamp: None,
        };
        h.with_ctx(|ctx| after_store(ctx, &incoming, &stored));
        item
    }

    fn row_of(h: &Harness, item: &str) -> queries::MessageRow {
        queries::find_by_timeline_id(h.store.conn(), h.account_id, item)
            .unwrap()
            .unwrap()
    }

    fn edit_of(target: &str, body: &str) -> Message {
        let mut m = Message::chat(None).with_body("".into(), body.into());
        m.payloads.push(Element::from(Replace {
            id: Id(target.into()),
        }));
        m
    }

    #[test]
    fn an_edit_that_runs_ahead_of_its_target_applies_later() {
        let mut h = Harness::new();
        // From the archive (it has a time): no new row, and the edit waits.
        assert!(deliver(&mut h, &edit_of("m1", "second"), Some(2000)));
        assert!(deliver(&mut h, &edit_of("m1", "third"), Some(3000)));
        assert_eq!(count(&h), 2);
        assert!(
            queries::messages_with(h.store.conn(), h.account_id, BOB)
                .unwrap()
                .is_empty()
        );
        let item = target_arrives(&mut h, "m1");
        assert_eq!(row_of(&h, &item).body, "third");
        assert_eq!(count(&h), 0);
    }

    #[test]
    fn a_live_edit_with_no_target_is_still_a_new_message() {
        let mut h = Harness::new();
        assert!(!deliver(&mut h, &edit_of("m1", "second"), None));
        assert_eq!(count(&h), 0);
    }

    #[test]
    fn a_retraction_that_runs_ahead_applies_later() {
        let mut h = Harness::new();
        let mut m = Message::chat(None);
        m.payloads.push(
            Element::builder("retract", "urn:xmpp:message-retract:1")
                .attr(nc("id"), "m1")
                .build(),
        );
        assert!(deliver(&mut h, &m, None));
        assert_eq!(count(&h), 1);
        let item = target_arrives(&mut h, "m1");
        assert!(row_of(&h, &item).retracted);
        assert_eq!(count(&h), 0);
    }

    #[test]
    fn a_reaction_that_runs_ahead_applies_later() {
        let mut h = Harness::new();
        let mut m = Message::chat(None);
        m.payloads.push(
            Element::builder("reactions", "urn:xmpp:reactions:0")
                .attr(nc("id"), "m1")
                .append(Element::builder("reaction", "urn:xmpp:reactions:0").append("+"))
                .build(),
        );
        assert!(deliver(&mut h, &m, None));
        assert_eq!(count(&h), 1);
        target_arrives(&mut h, "m1");
        let emojis: String = h
            .store
            .conn()
            .query_row("SELECT emojis FROM reactions", [], |r| r.get(0))
            .unwrap();
        assert!(emojis.contains('+'));
        assert_eq!(count(&h), 0);
    }

    #[test]
    fn a_waiting_change_of_another_sender_does_nothing_and_goes() {
        let mut h = Harness::new();
        let m = edit_of("m1", "evil");
        let incoming = Incoming {
            message: &m,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: BOB,
            sender: "eve@chord.localhost/x",
            timestamp: Some(5),
        };
        h.with_ctx(|ctx| super::super::message_ext::intercept(ctx, &incoming));
        assert_eq!(count(&h), 1);
        let item = target_arrives(&mut h, "m1");
        assert_eq!(row_of(&h, &item).body, "old");
        assert_eq!(count(&h), 0);
    }

    fn count(h: &Harness) -> i64 {
        h.store
            .conn()
            .query_row("SELECT COUNT(*) FROM pending_changes", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_change_that_is_old_goes_when_the_next_one_arrives() {
        let mut h = Harness::new();
        let message = Message::chat(None);
        let incoming = Incoming {
            message: &message,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: "bob@chord.localhost",
            sender: "bob@chord.localhost/x",
            timestamp: Some(1),
        };
        h.with_ctx(|ctx| stash(ctx, &incoming, "m1", Change::Edit));
        h.store
            .conn()
            .execute("UPDATE pending_changes SET created_at = 1", [])
            .unwrap();
        h.with_ctx(|ctx| stash(ctx, &incoming, "m2", Change::Edit));
        assert_eq!(count(&h), 1);
    }

    #[test]
    fn an_account_keeps_a_limited_number_of_changes() {
        let mut h = Harness::new();
        let message = Message::chat(None);
        let incoming = Incoming {
            message: &message,
            kind: MessageKind::Chat,
            direction: Direction::In,
            peer: "bob@chord.localhost",
            sender: "bob@chord.localhost/x",
            timestamp: None,
        };
        for i in 0..MAX_PENDING + 5 {
            h.with_ctx(|ctx| stash(ctx, &incoming, &format!("m{i}"), Change::Reaction));
        }
        assert_eq!(count(&h), MAX_PENDING);
        let oldest: String = h
            .store
            .conn()
            .query_row(
                "SELECT target FROM pending_changes ORDER BY id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(oldest, "m5");
    }
}
