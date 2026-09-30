//! User tune (XEP-0118): what a contact plays now, from PEP events.
//!
//! Chord reads tunes and shows them as a line "Artist - Title" on the contact. It does
//! not publish one: a desktop player has no common API for the current song. The store
//! keeps the line while the contact is online, and clears it with each new session (the
//! server sends the last item again after the presence with `+notify` caps).
//!
//! XEP-0108 (user activity, "Playing X") has no type in xmpp-parsers 0.23 and no stored
//! form yet. It is not read.

use jid::BareJid;
use rusqlite::params;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::pubsub::event::Payload;

use super::Ctx;

pub const NODE_TUNE: &str = "http://jabber.org/protocol/tune";
const NS_TUNE: &str = NODE_TUNE;

/// The longest line that Chord keeps.
const MAX_CHARS: usize = 200;

/// A tune event from `owner`. Our own account is not a contact: its tunes are dropped.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, owner: &BareJid, payload: Payload) {
    if owner == ctx.account {
        return;
    }
    let line = match payload {
        Payload::Items {
            published,
            retracted,
            ..
        } => match published.iter().rev().find_map(|i| i.payload.clone()) {
            Some(element) => line_of(&element),
            None if !retracted.is_empty() => None,
            None => return,
        },
        Payload::Purge { .. } | Payload::Delete { .. } => None,
        Payload::Configuration { .. } | Payload::Subscription { .. } => return,
    };
    let result = match &line {
        Some(line) => ctx.store.conn().execute(
            "INSERT INTO contact_tunes (account_id, bare, text) VALUES (?1, ?2, ?3)
             ON CONFLICT (account_id, bare) DO UPDATE SET text = excluded.text",
            params![ctx.account_id, owner.as_str(), line],
        ),
        None => ctx.store.conn().execute(
            "DELETE FROM contact_tunes WHERE account_id = ?1 AND bare = ?2",
            params![ctx.account_id, owner.as_str()],
        ),
    };
    if let Err(e) = result {
        ctx.store_error("store a tune", e);
        return;
    }
    super::roster::mark(ctx, owner.clone());
}

/// "Artist - Title" of a `<tune/>` element. An empty element means that the contact
/// stopped the music. With only one of the two, the line holds that one.
fn line_of(element: &Element) -> Option<String> {
    if !element.is("tune", NS_TUNE) {
        return None;
    }
    let text = |name: &str| {
        element
            .get_child(name, NS_TUNE)
            .map(|e| e.text().trim().to_owned())
            .filter(|t| !t.is_empty())
    };
    let line = match (text("artist"), text("title")) {
        (Some(artist), Some(title)) => format!("{artist} - {title}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => return None,
    };
    Some(line.chars().take(MAX_CHARS).collect())
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::pubsub::event::Payload;

    use super::*;
    use crate::features::testing::Harness;

    fn element(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    #[test]
    fn a_tune_becomes_one_line() {
        let tune = element(
            "<tune xmlns='http://jabber.org/protocol/tune'>\
             <artist>Yes</artist><title>Heart of the Sunrise</title><length>686</length></tune>",
        );
        assert_eq!(
            line_of(&tune).as_deref(),
            Some("Yes - Heart of the Sunrise")
        );
        let only_title =
            element("<tune xmlns='http://jabber.org/protocol/tune'><title>Untitled</title></tune>");
        assert_eq!(line_of(&only_title).as_deref(), Some("Untitled"));
        let stopped = element("<tune xmlns='http://jabber.org/protocol/tune'/>");
        assert_eq!(line_of(&stopped), None);
        assert_eq!(line_of(&element("<tune xmlns='other'/>")), None);
    }

    fn items(published: Option<&str>) -> Payload {
        use xmpp_parsers::pubsub::event::Item;
        use xmpp_parsers::pubsub::{ItemId, NodeName};
        Payload::Items {
            node: NodeName(NODE_TUNE.into()),
            published: published
                .map(|xml| Item {
                    id: Some(ItemId("current".into())),
                    publisher: None,
                    payload: Some(element(xml)),
                })
                .into_iter()
                .collect(),
            retracted: vec![],
        }
    }

    #[test]
    fn the_store_keeps_the_line_until_the_tune_stops_and_ignores_our_account() {
        let mut h = Harness::new();
        let bob = BareJid::new("bob@chord.localhost").unwrap();
        let play = "<tune xmlns='http://jabber.org/protocol/tune'><artist>A</artist><title>B</title></tune>";
        h.with_ctx(|ctx| on_event(ctx, &bob, items(Some(play))));
        let read = |h: &Harness| -> Option<String> {
            h.store
                .conn()
                .query_row(
                    "SELECT text FROM contact_tunes WHERE bare = ?1",
                    [bob.as_str()],
                    |r| r.get(0),
                )
                .ok()
        };
        assert_eq!(read(&h).as_deref(), Some("A - B"));
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                &bob,
                items(Some("<tune xmlns='http://jabber.org/protocol/tune'/>")),
            )
        });
        assert_eq!(read(&h), None);

        let me = h.account.clone();
        h.with_ctx(|ctx| on_event(ctx, &me, items(Some(play))));
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT count(*) FROM contact_tunes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
