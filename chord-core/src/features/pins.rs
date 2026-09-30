//! Pinned messages, kept in a private PEP node of our account (XEP-0223) and shared by all
//! clients of the account.
//!
//! Why PEP and not a bookmark node or a local table: a pin is data of the user, not of the
//! room, so every room and every 1:1 chat can have pins, including rooms where we are no
//! moderator. XEP-0223 is the recipe for private data in PEP: the node keeps its items and
//! only we can read it (`access_model` whitelist). The server sends a notification to our
//! other clients, which advertise the node with `+notify`. A bookmark node (XEP-0402) has
//! the shape of a room and is read by other clients, so it does not fit.
//!
//! The node is `urn:chord:pins:0`. Each item is one pin, with the id `<chat>/<key>` (a
//! bare JID has no "/"). One item for each pin means that two clients that pin at the same
//! time do not overwrite one another. The payload has a copy of the message, because
//! another device may not have the message in its store:
//!
//! ```xml
//! <pin xmlns='urn:chord:pins:0' chat='room@muc.example.org' id='STANZA-ID'
//!      sender='nick' ts='1700000000000' at='1700000500000'>the body</pin>
//! ```
//!
//! `chat` is the bare JID of the room or the contact, `id` is the stanza-id of the message
//! (or its origin-id, or its id, the first that we have), `ts` the time of the message and
//! `at` the time of the pin, both in Unix ms. The `pins` table keeps a copy for offline
//! reads.

use futures_channel::oneshot;
use rusqlite::params;
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::pubsub::pubsub::{Item, Items, PubSub, Publish, PublishOptions, Retract};
use xmpp_parsers::pubsub::{ItemId, NodeName};
use xmpp_parsers::stanza_error::DefinedCondition;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, muc};
use crate::actor::{ClientError, ClientHandle};
use crate::store::Store;
use crate::store::queries::{self, MessageKind};
use crate::views::ViewKey;

pub const NODE_PINS: &str = "urn:chord:pins:0";

/// The most characters of the body that a pin keeps.
const MAX_BODY_CHARS: usize = 500;

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// One pinned message.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct Pin {
    /// The bare JID of the room or the contact.
    pub chat: String,
    /// The stanza-id, origin-id, or id of the message. It is the same on every device.
    pub key: String,
    /// The id of the message in the timeline view, if this device has the message.
    pub item_id: Option<String>,
    /// The nick in a room, else the JID of the sender.
    pub sender: String,
    /// The start of the body, at most 500 characters.
    pub body: String,
    /// The time of the message, Unix ms.
    pub timestamp: i64,
    /// The time of the pin, Unix ms.
    pub pinned_at: i64,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The fetch at connect has no `reply`. `refresh_pins` has one.
    Fetch {
        reply: Option<Reply<()>>,
    },
    Publish {
        pin: Pin,
        reply: Reply<()>,
    },
    Retract {
        chat: String,
        key: String,
        reply: Reply<()>,
    },
}

/// A command from the public API.
pub(crate) enum Command {
    Pin {
        item_id: String,
        reply: Reply<()>,
    },
    Unpin {
        chat: String,
        key: String,
        reply: Reply<()>,
    },
    List {
        chat: Option<String>,
        reply: Reply<Vec<Pin>>,
    },
    Refresh {
        reply: Reply<()>,
    },
}

impl ClientHandle {
    /// Pin a message. `item_id` is the `TimelineItem::id` of the message. The pin goes to a
    /// private PEP node of our account, so our other clients see it too (XEP-0223). Pinning
    /// a message twice changes nothing.
    ///
    /// Fails with `Invalid` for an unknown message and for one that has no id that other
    /// devices can use yet. Fails with `Server` if the server refuses the node.
    pub async fn pin_message(&self, item_id: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Pins(Command::Pin { item_id, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Remove a pin. `chat` and `key` are the fields of a `Pin`. A pin that is gone already
    /// is no error.
    pub async fn unpin_message(&self, chat: String, key: String) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Pins(Command::Unpin { chat, key, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Ask the server for the pins again and replace the copy. The answer comes when the
    /// copy is new. The core does this at each connect, and a PEP event keeps the copy
    /// current, so this is for a caller that needs to be sure (the CLI, a pull to refresh).
    pub async fn refresh_pins(&self) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Pins(Command::Refresh { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The pins of one chat or room (a bare JID), or of all chats. The newest pin is first.
    /// It reads the store, so it works offline.
    pub async fn pins(&self, chat: Option<String>) -> Result<Vec<Pin>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Pins(Command::List { chat, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

fn item_id_of(chat: &str, key: &str) -> String {
    format!("{chat}/{key}")
}

/// Unix time in ms from SQLite's clock, because `SystemTime::now` panics in the WASM build.
fn now_ms(store: &Store) -> i64 {
    store
        .conn()
        .query_row(
            "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
}

fn store_error(e: rusqlite::Error) -> ClientError {
    ClientError::Invalid(format!("store: {e}"))
}

impl Pin {
    fn element(&self) -> Element {
        Element::builder("pin", NODE_PINS)
            .attr(nc("chat"), self.chat.as_str())
            .attr(nc("id"), self.key.as_str())
            .attr(nc("sender"), self.sender.as_str())
            .attr(nc("ts"), self.timestamp.to_string())
            .attr(nc("at"), self.pinned_at.to_string())
            .append(self.body.as_str())
            .build()
    }

    /// The pin in an item. `None` for an item that is not valid.
    fn parse(payload: Option<&Element>) -> Option<Self> {
        let e = payload.filter(|e| e.is("pin", NODE_PINS))?;
        let chat = e.attr("chat").filter(|c| !c.is_empty())?;
        let key = e.attr("id").filter(|k| !k.is_empty())?;
        Some(Self {
            chat: chat.to_owned(),
            key: key.to_owned(),
            item_id: None,
            sender: e.attr("sender").unwrap_or_default().to_owned(),
            body: e.text().chars().take(MAX_BODY_CHARS).collect(),
            timestamp: e.attr("ts").and_then(|v| v.parse().ok()).unwrap_or(0),
            pinned_at: e.attr("at").and_then(|v| v.parse().ok()).unwrap_or(0),
        })
    }
}

/// Ask for the pins. The answer replaces the copy in the store.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    fetch(ctx, None);
}

fn fetch(ctx: &mut Ctx<'_>, reply: Option<Reply<()>>) {
    let iq = Iq::from_get("", PubSub::Items(Items::new(NODE_PINS)));
    ctx.request(iq, FeaturePending::Pins(Pending::Fetch { reply }));
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Pin { item_id, reply } => pin(ctx, &item_id, reply),
        Command::Unpin { chat, key, reply } => unpin(ctx, chat, key, reply),
        Command::Refresh { reply } => fetch(ctx, Some(reply)),
        Command::List { chat, reply } => {
            let _ = reply.send(list(ctx.store, ctx.account_id, chat.as_deref()));
        }
    }
}

/// A command while no session is up. The list reads the store. The rest need the server.
pub(crate) fn offline(store: &Store, account_id: i64, command: Command) {
    match command {
        Command::List { chat, reply } => {
            let _ = reply.send(list(store, account_id, chat.as_deref()));
        }
        Command::Pin { reply, .. } | Command::Unpin { reply, .. } | Command::Refresh { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// Publish the pin of a stored message.
fn pin(ctx: &mut Ctx<'_>, item_id: &str, reply: Reply<()>) {
    let pin = match pin_of(ctx, item_id) {
        Ok(pin) => pin,
        Err(e) => {
            let _ = reply.send(Err(e));
            return;
        }
    };
    let item = Item {
        id: Some(ItemId(item_id_of(&pin.chat, &pin.key))),
        publisher: None,
        payload: Some(pin.element()),
    };
    // XEP-0223: the node keeps its items, and only we can read it.
    let options = DataForm::new(
        DataFormType::Submit,
        "http://jabber.org/protocol/pubsub#publish-options",
        vec![
            Field::new("pubsub#persist_items", FieldType::Boolean).with_value("true"),
            Field::new("pubsub#max_items", FieldType::TextSingle).with_value("max"),
            Field::new("pubsub#send_last_published_item", FieldType::ListSingle)
                .with_value("never"),
            Field::new("pubsub#access_model", FieldType::ListSingle).with_value("whitelist"),
        ],
    );
    let iq = Iq::from_set(
        "",
        PubSub::Publish {
            publish: Publish {
                node: NodeName(NODE_PINS.to_owned()),
                items: vec![item],
            },
            publish_options: Some(PublishOptions {
                form: Some(options),
            }),
        },
    );
    ctx.request(iq, FeaturePending::Pins(Pending::Publish { pin, reply }));
}

/// The pin for a stored message.
fn pin_of(ctx: &Ctx<'_>, item_id: &str) -> Result<Pin, ClientError> {
    let row = queries::find_by_timeline_id(ctx.store.conn(), ctx.account_id, item_id)
        .map_err(store_error)?
        .ok_or_else(|| ClientError::Invalid(format!("no message {item_id}")))?;
    if row.retracted {
        return Err(ClientError::Invalid("the message is retracted".into()));
    }
    // Another device must find the message by this key, so prefer the ids that the server
    // gave over the id that we made.
    let key = row
        .stanza_id
        .clone()
        .or_else(|| row.origin_id.clone())
        .or_else(|| row.message_id.clone())
        .ok_or_else(|| ClientError::Invalid("the message has no id yet".into()))?;
    let sender = match row.kind {
        MessageKind::Groupchat => row
            .sender
            .split_once('/')
            .map_or(row.sender.as_str(), |(_, nick)| nick)
            .to_owned(),
        MessageKind::Chat => row.sender.clone(),
    };
    Ok(Pin {
        chat: row.peer,
        key,
        item_id: Some(item_id.to_owned()),
        sender,
        body: row.body.chars().take(MAX_BODY_CHARS).collect(),
        timestamp: row.timestamp,
        pinned_at: now_ms(ctx.store),
    })
}

fn unpin(ctx: &mut Ctx<'_>, chat: String, key: String, reply: Reply<()>) {
    let item = Item {
        id: Some(ItemId(item_id_of(&chat, &key))),
        publisher: None,
        payload: None,
    };
    let iq = Iq::from_set(
        "",
        PubSub::Retract(Retract {
            node: NodeName(NODE_PINS.to_owned()),
            notify: true,
            items: vec![item],
        }),
    );
    ctx.request(
        iq,
        FeaturePending::Pins(Pending::Retract { chat, key, reply }),
    );
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Fetch { reply } => {
            let result = on_fetched(ctx, response);
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        Pending::Publish { pin, reply } => {
            let result = match response {
                IqResponse::Result(_) => {
                    upsert(ctx, &pin);
                    Ok(())
                }
                IqResponse::Error(e) => Err(ClientError::Server(muc::error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            let _ = reply.send(result);
        }
        Pending::Retract { chat, key, reply } => {
            let result = match response {
                IqResponse::Result(_) => Ok(()),
                // The pin is gone already, or the node was never made.
                IqResponse::Error(e)
                    if matches!(e.defined_condition, DefinedCondition::ItemNotFound) =>
                {
                    Ok(())
                }
                IqResponse::Error(e) => Err(ClientError::Server(muc::error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            if result.is_ok() {
                remove(ctx, &chat, &key);
            }
            let _ = reply.send(result);
        }
    }
}

fn on_fetched(ctx: &mut Ctx<'_>, response: IqResponse) -> Result<(), ClientError> {
    let pins = match response {
        IqResponse::Result(Some(payload)) => match PubSub::try_from(payload) {
            Ok(PubSub::Items(items)) => items
                .items
                .iter()
                .filter_map(|i| Pin::parse(i.payload.as_ref()))
                .collect::<Vec<_>>(),
            _ => {
                log::warn!("the pins answer is not an items list");
                return Err(ClientError::Server(
                    "the pins answer is not an items list".into(),
                ));
            }
        },
        IqResponse::Result(None) => Vec::new(),
        // No node yet: there are no pins.
        IqResponse::Error(e) if matches!(e.defined_condition, DefinedCondition::ItemNotFound) => {
            Vec::new()
        }
        IqResponse::Error(e) => {
            // Keep the copy.
            let text = muc::error_text(&e);
            log::warn!("cannot fetch pins: {text}");
            return Err(ClientError::Server(text));
        }
        IqResponse::Lost => return Err(ClientError::NotConnected),
    };
    replace_all(ctx, &pins).map_err(|e| {
        ctx.store_error("store the pins", e);
        ClientError::Invalid("cannot store the pins".into())
    })
}

/// A PEP event on our pins node: a pin was added or removed on another device.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, payload: Payload) {
    match payload {
        Payload::Items {
            published,
            retracted,
            ..
        } => {
            for item in &published {
                if let Some(pin) = Pin::parse(item.payload.as_ref()) {
                    upsert(ctx, &pin);
                }
            }
            for id in retracted {
                if let Some((chat, key)) = id.0.split_once('/') {
                    remove(ctx, chat, key);
                }
            }
        }
        Payload::Purge { .. } | Payload::Delete { .. } => {
            if let Err(e) = replace_all(ctx, &[]) {
                ctx.store_error("clear the pins", e);
            }
        }
        Payload::Configuration { .. } | Payload::Subscription { .. } => {}
    }
}

fn upsert(ctx: &Ctx<'_>, pin: &Pin) {
    let result = ctx.store.conn().execute(
        "INSERT INTO pins (account_id, chat, key, sender, body, timestamp, pinned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (account_id, chat, key) DO UPDATE SET
            sender = excluded.sender, body = excluded.body,
            timestamp = excluded.timestamp, pinned_at = excluded.pinned_at",
        params![
            ctx.account_id,
            pin.chat,
            pin.key,
            pin.sender,
            pin.body,
            pin.timestamp,
            pin.pinned_at
        ],
    );
    if let Err(e) = result {
        ctx.store_error("store a pin", e);
    }
}

fn remove(ctx: &Ctx<'_>, chat: &str, key: &str) {
    let result = ctx.store.conn().execute(
        "DELETE FROM pins WHERE account_id = ?1 AND chat = ?2 AND key = ?3",
        params![ctx.account_id, chat, key],
    );
    if let Err(e) = result {
        ctx.store_error("remove a pin", e);
    }
}

/// Make the table match the list.
fn replace_all(ctx: &mut Ctx<'_>, pins: &[Pin]) -> rusqlite::Result<()> {
    ctx.store.conn().execute(
        "DELETE FROM pins WHERE account_id = ?1",
        params![ctx.account_id],
    )?;
    for pin in pins {
        upsert(ctx, pin);
    }
    ctx.changed(ViewKey::All);
    Ok(())
}

/// The pins of one chat, or of all chats, the newest pin first.
pub(crate) fn list(
    store: &Store,
    account_id: i64,
    chat: Option<&str>,
) -> Result<Vec<Pin>, ClientError> {
    let mut stmt = store
        .conn()
        .prepare(
            "SELECT chat, key, sender, body, timestamp, pinned_at FROM pins
             WHERE account_id = ?1 AND (?2 IS NULL OR chat = ?2)
             ORDER BY pinned_at DESC, key",
        )
        .map_err(store_error)?;
    let mut pins = stmt
        .query_map(params![account_id, chat], |r| {
            Ok(Pin {
                chat: r.get(0)?,
                key: r.get(1)?,
                item_id: None,
                sender: r.get(2)?,
                body: r.get(3)?,
                timestamp: r.get(4)?,
                pinned_at: r.get(5)?,
            })
        })
        .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
        .map_err(store_error)?;
    for pin in &mut pins {
        // This device may not have the message. Then the pin has no link to it.
        pin.item_id = queries::find_message(store.conn(), account_id, &pin.chat, &pin.key)
            .map_err(store_error)?
            .map(|row| format!("m:{}", row.rowid));
    }
    Ok(pins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Pending as P;
    use crate::features::testing::Harness;
    use xmpp_parsers::stanza_error::{ErrorType, StanzaError};

    const ROOM: &str = "dev@rooms.chord.localhost";

    fn is_pins(p: &P) -> bool {
        matches!(p, P::Pins(_))
    }

    fn sample(key: &str, at: i64) -> Pin {
        Pin {
            chat: ROOM.into(),
            key: key.into(),
            item_id: None,
            sender: "bob".into(),
            body: "remember this".into(),
            timestamp: 1_000,
            pinned_at: at,
        }
    }

    fn items_result(pins: &[Pin]) -> Element {
        let mut node = Element::builder("items", "http://jabber.org/protocol/pubsub")
            .attr(nc("node"), NODE_PINS)
            .build();
        for pin in pins {
            node.append_child(
                Element::builder("item", "http://jabber.org/protocol/pubsub")
                    .attr(nc("id"), item_id_of(&pin.chat, &pin.key))
                    .append(pin.element())
                    .build(),
            );
        }
        Element::builder("pubsub", "http://jabber.org/protocol/pubsub")
            .append(node)
            .build()
    }

    #[test]
    fn a_pin_survives_its_xml() {
        let pin = sample("s1", 5);
        assert_eq!(Pin::parse(Some(&pin.element())), Some(pin));
        // A payload with another name, or without a chat, is no pin.
        let other: Element = "<pin xmlns='urn:chord:pins:0' id='x'>hi</pin>"
            .parse()
            .unwrap();
        assert_eq!(Pin::parse(Some(&other)), None);
        assert_eq!(Pin::parse(None), None);
    }

    #[test]
    fn a_long_body_is_cut() {
        let mut pin = sample("s1", 5);
        pin.body = "x".repeat(MAX_BODY_CHARS + 20);
        let parsed = Pin::parse(Some(&pin.element())).unwrap();
        assert_eq!(parsed.body.chars().count(), MAX_BODY_CHARS);
    }

    #[test]
    fn connecting_asks_for_the_pins_and_the_answer_replaces_the_copy() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| upsert(ctx, &sample("old", 1)));
        h.with_ctx(on_connected);
        let iqs = h.sent_iqs();
        assert!(matches!(&iqs[0], Iq::Get { .. }));
        assert!(iqs[0].to().is_none(), "sent to our own account");
        h.answer(
            is_pins,
            Some(items_result(&[sample("s1", 5), sample("s2", 9)])),
        );
        let pins = list(&h.store, h.account_id, Some(ROOM)).unwrap();
        let keys: Vec<_> = pins.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            ["s2", "s1"],
            "the newest first, and the old copy is gone"
        );
    }

    #[test]
    fn a_missing_node_means_no_pins() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| upsert(ctx, &sample("old", 1)));
        h.with_ctx(on_connected);
        let error = StanzaError::new(ErrorType::Cancel, DefinedCondition::ItemNotFound, "en", "");
        h.respond(is_pins, IqResponse::Error(error));
        assert!(list(&h.store, h.account_id, None).unwrap().is_empty());
    }

    #[test]
    fn events_from_another_device_add_and_remove_pins() {
        let mut h = Harness::new();
        let published = xmpp_parsers::pubsub::event::Item {
            id: Some(ItemId(item_id_of(ROOM, "s1"))),
            publisher: None,
            payload: Some(sample("s1", 5).element()),
        };
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                Payload::Items {
                    node: NodeName(NODE_PINS.into()),
                    published: vec![published],
                    retracted: vec![],
                },
            )
        });
        assert_eq!(list(&h.store, h.account_id, None).unwrap().len(), 1);
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                Payload::Items {
                    node: NodeName(NODE_PINS.into()),
                    published: vec![],
                    retracted: vec![ItemId(item_id_of(ROOM, "s1"))],
                },
            )
        });
        assert!(list(&h.store, h.account_id, None).unwrap().is_empty());
    }

    #[test]
    fn pinning_publishes_to_a_private_node_then_stores_and_unpinning_retracts() {
        use crate::store::queries::{Direction, KeyKind, MessageExtras, NewMessage};
        let mut h = Harness::new();
        let stored = queries::insert_message(
            h.store.conn(),
            h.account_id,
            &NewMessage {
                kind: MessageKind::Groupchat,
                key_kind: KeyKind::StanzaId,
                key: "sid-1",
                direction: Direction::In,
                peer: ROOM,
                sender: &format!("{ROOM}/bob"),
                body: "remember this",
                timestamp: Some(1_000),
                extras: MessageExtras::default(),
            },
        )
        .unwrap()
        .unwrap();
        let item_id = format!("m:{}", stored.rowid);

        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| pin(ctx, &item_id, reply));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set: {:?}", iqs[0]);
        };
        let xml = String::from(payload);
        assert!(xml.contains(&format!("node='{NODE_PINS}'")), "{xml}");
        assert!(xml.contains(&format!("id='{ROOM}/sid-1'")), "{xml}");
        assert!(
            xml.contains("whitelist"),
            "only we can read the node: {xml}"
        );
        assert!(
            list(&h.store, h.account_id, None).unwrap().is_empty(),
            "not before the answer"
        );
        h.answer(is_pins, None);
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        let pins = list(&h.store, h.account_id, Some(ROOM)).unwrap();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].sender, "bob", "a room pin has the nick");
        assert_eq!(pins[0].key, "sid-1");
        assert_eq!(pins[0].item_id.as_deref(), Some(item_id.as_str()));

        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| unpin(ctx, ROOM.into(), "sid-1".into(), reply));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert!(String::from(payload).contains("retract"));
        h.answer(is_pins, None);
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        assert!(list(&h.store, h.account_id, None).unwrap().is_empty());
    }

    #[test]
    fn a_message_without_an_id_or_a_retracted_one_cannot_be_pinned() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| pin(ctx, "m:999", reply));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Invalid(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn the_list_is_per_account_and_per_chat() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            upsert(ctx, &sample("a", 1));
            let mut other = sample("b", 2);
            other.chat = "bob@chord.localhost".into();
            upsert(ctx, &other);
        });
        assert_eq!(list(&h.store, h.account_id, Some(ROOM)).unwrap().len(), 1);
        assert_eq!(list(&h.store, h.account_id, None).unwrap().len(), 2);
        assert!(list(&h.store, h.account_id + 1, None).unwrap().is_empty());
    }
}
