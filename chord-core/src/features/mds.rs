//! Message Displayed Synchronization (XEP-0490): the read state of all our devices.
//!
//! A chat marker (XEP-0333) reaches only the devices that are online. MDS keeps the read
//! position of each chat in our own PEP node `urn:xmpp:mds:displayed:0`, so a device that
//! was offline, or a new device, learns what another device read.
//!
//! - The item id is the bare JID of the chat or of the room. The item holds the stanza-id
//!   of the newest message that we read: `<displayed><stanza-id id= by=/></displayed>`. For
//!   a 1:1 chat `by` is our own account, and for a room it is the room.
//! - Publish: `markers.rs` calls `publish` when `mark_read` sends its marker. A private
//!   message of a room has no item, because the item id is a bare JID.
//! - Read: at each new session we read the node, and we advertise `+notify`, so the server
//!   sends each later change. Both give a chat JID and a stanza-id.
//! - Apply: if the message with that stanza-id is in the store, its row becomes the read
//!   position (never back). If not, the stanza-id waits in `State::waiting`. `after_store`
//!   applies it when that message arrives, for example from the archive. The state is not
//!   stored, so after a restart the next session reads the node again.

use std::collections::HashMap;

use jid::BareJid;
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::ns;
use xmpp_parsers::pubsub::event::Payload;

use super::{Ctx, IqResponse, Pending as FeaturePending, markers};
use crate::store::queries::{KeyKind, MessageKind, StoredMessage};

/// The PEP node of XEP-0490, and the namespace of its items.
pub const NODE_MDS: &str = "urn:xmpp:mds:displayed:0";
const NS_SID: &str = "urn:xmpp:sid:0";
const NS_DATA: &str = "jabber:x:data";
const FORM_PUBLISH_OPTIONS: &str = "http://jabber.org/protocol/pubsub#publish-options";

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

#[derive(Debug, Default)]
pub(crate) struct State {
    /// The stanza-id of the newest message that another device read, by chat JID. It waits
    /// for a message that the store does not have yet.
    waiting: HashMap<String, String>,
}

#[derive(Debug)]
pub(crate) enum Pending {
    /// Our read of the node at the start of the session.
    Items,
    /// A publish. With `options`, a refusal makes us publish again without the options.
    Publish {
        peer: String,
        stanza_id: String,
        by: BareJid,
        options: bool,
    },
}

/// A new session is up: read the node. Our own PEP answers without a `to`.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    ctx.state.mds.waiting.clear();
    let items = Element::builder("items", ns::PUBSUB).attr(nc("node"), NODE_MDS);
    let iq = Iq::Get {
        from: None,
        to: None,
        id: String::new(),
        payload: Element::builder("pubsub", ns::PUBSUB).append(items).build(),
    };
    ctx.request(iq, FeaturePending::Mds(Pending::Items));
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Items => match response {
            IqResponse::Result(Some(payload)) => {
                let Some(items) = payload.get_child("items", ns::PUBSUB) else {
                    return;
                };
                let found: Vec<(String, Element)> = items
                    .children()
                    .filter(|i| i.is("item", ns::PUBSUB))
                    .filter_map(|i| Some((i.attr("id")?.to_owned(), i.children().next()?.clone())))
                    .collect();
                for (peer, displayed) in found {
                    apply_item(ctx, &peer, &displayed);
                }
            }
            other => log::debug!("MDS read: {other:?}"),
        },
        Pending::Publish {
            peer,
            stanza_id,
            by,
            options,
        } => match response {
            IqResponse::Result(_) | IqResponse::Lost => {}
            IqResponse::Error(e) if options => {
                // The service may refuse the options (for example `max_items` of `max`).
                log::info!(
                    "MDS publish with options failed ({:?}). Trying without.",
                    e.defined_condition
                );
                send_publish(ctx, &peer, &stanza_id, &by, false);
            }
            IqResponse::Error(e) => {
                log::warn!("MDS publish for {peer} failed: {:?}", e.defined_condition);
            }
        },
    }
}

/// Tell our other devices that we read `peer` up to the message with `stanza_id`. `peer`
/// must be a bare JID. `kind` decides who added the stanza-id: our server or the room.
pub(crate) fn publish(ctx: &mut Ctx<'_>, peer: &str, kind: MessageKind, stanza_id: &str) {
    let Ok(chat) = BareJid::new(peer) else {
        return;
    };
    let by = match kind {
        MessageKind::Chat => ctx.account.clone(),
        MessageKind::Groupchat => chat,
    };
    send_publish(ctx, peer, stanza_id, &by, true);
}

fn send_publish(ctx: &mut Ctx<'_>, peer: &str, stanza_id: &str, by: &BareJid, options: bool) {
    let stanza = Element::builder("stanza-id", NS_SID)
        .attr(nc("id"), stanza_id)
        .attr(nc("by"), by.as_str());
    let displayed = Element::builder("displayed", NODE_MDS).append(stanza);
    let item = Element::builder("item", ns::PUBSUB)
        .attr(nc("id"), peer)
        .append(displayed);
    let mut pubsub = Element::builder("pubsub", ns::PUBSUB).append(
        Element::builder("publish", ns::PUBSUB)
            .attr(nc("node"), NODE_MDS)
            .append(item),
    );
    if options {
        let field = |var: &str, value: &str| {
            Element::builder("field", NS_DATA)
                .attr(nc("var"), var)
                .append(Element::builder("value", NS_DATA).append(value))
        };
        let form = Element::builder("x", NS_DATA)
            .attr(nc("type"), "submit")
            .append(field("FORM_TYPE", FORM_PUBLISH_OPTIONS))
            .append(field("pubsub#persist_items", "true"))
            .append(field("pubsub#max_items", "max"))
            .append(field("pubsub#send_last_published_item", "never"))
            .append(field("pubsub#access_model", "whitelist"));
        pubsub = pubsub.append(Element::builder("publish-options", ns::PUBSUB).append(form));
    }
    let iq = Iq::Set {
        from: None,
        to: None,
        id: String::new(),
        payload: pubsub.build(),
    };
    ctx.request_publish(
        iq,
        FeaturePending::Mds(Pending::Publish {
            peer: peer.to_owned(),
            stanza_id: stanza_id.to_owned(),
            by: by.clone(),
            options,
        }),
    );
}

/// A pubsub event of the MDS node from our own account: another device read a chat.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, payload: Payload) {
    let Payload::Items { published, .. } = payload else {
        return;
    };
    for item in published {
        let (Some(id), Some(displayed)) = (item.id, item.payload) else {
            continue;
        };
        apply_item(ctx, &id.0, &displayed);
    }
}

/// One item: the chat JID and its `<displayed/>` payload.
fn apply_item(ctx: &mut Ctx<'_>, peer: &str, displayed: &Element) {
    if !displayed.is("displayed", NODE_MDS) {
        return;
    }
    let Some(stanza_id) = displayed
        .get_child("stanza-id", NS_SID)
        .and_then(|s| s.attr("id"))
    else {
        return;
    };
    // Only a bare JID names a chat or a room. The item id is data from the server.
    let Ok(peer) = BareJid::new(peer) else {
        return;
    };
    let peer = peer.to_string();
    log::debug!("MDS: another device read {peer} up to {stanza_id}");
    let found = ctx
        .store
        .conn()
        .query_row(
            "SELECT id, kind FROM messages
             WHERE account_id = ?1 AND peer = ?2 AND stanza_id = ?3 AND direction = 'in'",
            params![ctx.account_id, peer, stanza_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional();
    match found {
        Ok(Some((rowid, kind))) => {
            let kind = MessageKind::parse(&kind).unwrap_or(MessageKind::Chat);
            markers::apply_remote_read(ctx, &peer, rowid, kind);
        }
        Ok(None) => {
            log::debug!("MDS: {stanza_id} of {peer} is not in the store yet");
            ctx.state.mds.waiting.insert(peer, stanza_id.to_owned());
        }
        Err(e) => ctx.store_error("find a message for MDS", e),
    }
}

/// A new message is in the store. If another device read it, apply that now.
pub(crate) fn on_stored(ctx: &mut Ctx<'_>, stored: &StoredMessage) {
    if ctx.state.mds.waiting.is_empty() || stored.key_kind != KeyKind::StanzaId {
        return;
    }
    if ctx.state.mds.waiting.get(&stored.peer) == Some(&stored.key) {
        ctx.state.mds.waiting.remove(&stored.peer);
        markers::apply_remote_read(ctx, &stored.peer, stored.rowid, stored.kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::store::queries::{Direction, MessageExtras, NewMessage, insert_message};
    use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

    const PEER: &str = "bob@chord.localhost";
    const ROOM: &str = "dev@rooms.chord.localhost";

    fn put(h: &Harness, peer: &str, sid: &str, kind: MessageKind) -> i64 {
        let new = NewMessage {
            kind,
            key_kind: KeyKind::StanzaId,
            key: sid,
            direction: Direction::In,
            peer,
            sender: "bob@chord.localhost/x",
            body: "hi",
            timestamp: None,
            extras: MessageExtras {
                stanza_id: Some(sid.into()),
                ..Default::default()
            },
        };
        insert_message(h.store.conn(), h.account_id, &new)
            .unwrap()
            .unwrap()
            .rowid
    }

    fn last_read(h: &Harness, peer: &str) -> Option<i64> {
        h.store
            .conn()
            .query_row(
                "SELECT last_read FROM read_state WHERE account_id = ?1 AND peer = ?2",
                params![h.account_id, peer],
                |r| r.get(0),
            )
            .optional()
            .unwrap()
    }

    fn is_items(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Mds(Pending::Items))
    }

    fn is_publish(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Mds(Pending::Publish { .. }))
    }

    fn item(peer: &str, sid: &str) -> String {
        format!(
            "<item id='{peer}'><displayed xmlns='{NODE_MDS}'>
               <stanza-id xmlns='{NS_SID}' id='{sid}' by='x'/></displayed></item>"
        )
    }

    fn result(items: &str) -> Element {
        format!(
            "<pubsub xmlns='{}'><items node='{NODE_MDS}'>{items}</items></pubsub>",
            ns::PUBSUB
        )
        .parse()
        .unwrap()
    }

    #[test]
    fn start_reads_the_node_and_moves_the_read_position() {
        let mut h = Harness::new();
        let first = put(&h, PEER, "s1", MessageKind::Chat);
        let second = put(&h, PEER, "s2", MessageKind::Chat);
        let room = put(&h, ROOM, "r1", MessageKind::Groupchat);
        h.with_ctx(on_connected);
        let sent = h.sent_iqs();
        let [Iq::Get { to, payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert!(to.is_none());
        assert_eq!(
            payload.get_child("items", ns::PUBSUB).unwrap().attr("node"),
            Some(NODE_MDS)
        );
        h.answer(
            is_items,
            Some(result(&format!("{}{}", item(PEER, "s1"), item(ROOM, "r1")))),
        );
        assert_eq!(last_read(&h, PEER), Some(first));
        assert_eq!(last_read(&h, ROOM), Some(room));
        // A newer read moves forward. An older one never moves back.
        let payload = result(&item(PEER, "s2"));
        h.with_ctx(|ctx| on_response(ctx, Pending::Items, IqResponse::Result(Some(payload))));
        assert_eq!(last_read(&h, PEER), Some(second));
        let payload = result(&item(PEER, "s1"));
        h.with_ctx(|ctx| on_response(ctx, Pending::Items, IqResponse::Result(Some(payload))));
        assert_eq!(last_read(&h, PEER), Some(second));
    }

    #[test]
    fn a_message_that_arrives_later_gets_the_read_position() {
        let mut h = Harness::new();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.answer(is_items, Some(result(&item(PEER, "s9"))));
        assert_eq!(last_read(&h, PEER), None);
        let rowid = put(&h, PEER, "s9", MessageKind::Chat);
        let stored = StoredMessage {
            rowid,
            kind: MessageKind::Chat,
            key_kind: KeyKind::StanzaId,
            key: "s9".into(),
            direction: Direction::In,
            peer: PEER.into(),
            sender: "bob@chord.localhost/x".into(),
            body: "hi".into(),
            timestamp: 0,
        };
        h.with_ctx(|ctx| on_stored(ctx, &stored));
        assert_eq!(last_read(&h, PEER), Some(rowid));
        // A second message does not apply again.
        assert!(h.state.mds.waiting.is_empty());
    }

    #[test]
    fn an_event_moves_the_read_position_and_bad_items_are_ignored() {
        let mut h = Harness::new();
        let rowid = put(&h, PEER, "s1", MessageKind::Chat);
        let event = |id: &str, inner: &str| -> Payload {
            let xml = format!(
                "<items xmlns='http://jabber.org/protocol/pubsub#event' node='{NODE_MDS}'>
                   <item id='{id}'>{inner}</item></items>"
            );
            let el: Element = xml.parse().unwrap();
            let event = xmpp_parsers::pubsub::event::Event::try_from(
                format!(
                    "<event xmlns='http://jabber.org/protocol/pubsub#event'>{}</event>",
                    String::from(&el)
                )
                .parse::<Element>()
                .unwrap(),
            )
            .unwrap();
            event.payload
        };
        let ok = format!(
            "<displayed xmlns='{NODE_MDS}'><stanza-id xmlns='{NS_SID}' id='s1' by='x'/></displayed>"
        );
        // An item id that is no bare JID, and an item with no stanza-id.
        h.with_ctx(|ctx| on_event(ctx, event("not a jid/", &ok)));
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                event(PEER, &format!("<displayed xmlns='{NODE_MDS}'/>")),
            )
        });
        assert_eq!(last_read(&h, PEER), None);
        h.with_ctx(|ctx| on_event(ctx, event(PEER, &ok)));
        assert_eq!(last_read(&h, PEER), Some(rowid));
        assert!(h.take_dirty().contains(&crate::views::ViewKey::ChannelList(
            crate::views::ChannelScope::Home
        )));
    }

    #[test]
    fn publish_sends_the_item_with_options_and_retries_without() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| publish(ctx, PEER, MessageKind::Chat, "s1"));
        let sent = h.sent_iqs();
        let [Iq::Set { to, payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert!(to.is_none());
        let publish = payload.get_child("publish", ns::PUBSUB).unwrap();
        assert_eq!(publish.attr("node"), Some(NODE_MDS));
        let item = publish.get_child("item", ns::PUBSUB).unwrap();
        assert_eq!(item.attr("id"), Some(PEER));
        let sid = item
            .get_child("displayed", NODE_MDS)
            .unwrap()
            .get_child("stanza-id", NS_SID)
            .unwrap();
        assert_eq!(sid.attr("id"), Some("s1"));
        assert_eq!(sid.attr("by"), Some("alice@chord.localhost"));
        assert!(payload.get_child("publish-options", ns::PUBSUB).is_some());
        // The service refuses the options: a second publish has none.
        h.respond(
            is_publish,
            IqResponse::Error(StanzaError::new(
                ErrorType::Modify,
                DefinedCondition::NotAcceptable,
                "en",
                "",
            )),
        );
        let sent = h.sent_iqs();
        let [Iq::Set { payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert!(payload.get_child("publish-options", ns::PUBSUB).is_none());
        assert!(payload.get_child("publish", ns::PUBSUB).is_some());
        h.respond(
            is_publish,
            IqResponse::Error(StanzaError::new(
                ErrorType::Modify,
                DefinedCondition::NotAcceptable,
                "en",
                "",
            )),
        );
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_room_item_names_the_room_as_the_stanza_id_author_and_a_private_chat_has_none() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| publish(ctx, ROOM, MessageKind::Groupchat, "r1"));
        let sent = h.sent_iqs();
        let [Iq::Set { payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        let text = String::from(payload);
        assert!(text.contains(&format!("by=\"{ROOM}\"")) || text.contains(&format!("by='{ROOM}'")));
        h.with_ctx(|ctx| publish(ctx, &format!("{ROOM}/nick"), MessageKind::Chat, "p1"));
        assert!(h.sent_iqs().is_empty());
    }
}
