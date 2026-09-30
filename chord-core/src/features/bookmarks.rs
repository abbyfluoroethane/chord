//! Bookmarks (XEP-0402): the list of rooms, shared by all clients of the account.
//!
//! The bookmarks live in the PEP node `urn:xmpp:bookmarks:1` of our account. Each item id
//! is a room JID. The `rooms` table keeps a copy. A local change (`add`, `remove`) only
//! changes the bookmark. A change from another client joins or leaves by its autojoin flag.

use futures_channel::oneshot;
use jid::BareJid;
use rusqlite::params;
use xmpp_parsers::bookmarks2::{Conference, Extensions};
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::jid::ResourcePart;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::pubsub::event::Payload;
use xmpp_parsers::pubsub::pubsub::{Item, Items, PubSub, Publish, PublishOptions, Retract};
use xmpp_parsers::pubsub::{ItemId, NodeName};
use xmpp_parsers::stanza_error::DefinedCondition;

use super::pubsub::NODE_BOOKMARKS;

const NS_BOOKMARKS2: &str = "urn:xmpp:bookmarks:1";
use super::{Ctx, IqResponse, Pending as FeaturePending, muc};
use crate::actor::ClientError;
use crate::views::ViewKey;

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    Fetch,
    Publish { bookmark: Bookmark, reply: Reply },
    Retract { room: BareJid, reply: Reply },
}

/// One bookmark.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Bookmark {
    pub room: BareJid,
    pub name: Option<String>,
    pub autojoin: bool,
    pub nick: Option<String>,
    pub password: Option<String>,
    /// The elements of the `<extensions/>` child that other clients wrote (XEP-0402). `None`
    /// when the bookmark has no such child. Chord keeps them and writes them back.
    pub extensions: Option<Vec<Element>>,
}

impl Bookmark {
    /// The bookmark in an item. `None` for an item that is not valid.
    fn parse(id: Option<&ItemId>, payload: Option<&Element>) -> Option<Self> {
        let room = BareJid::new(&id?.0).ok()?;
        let conference = match Conference::try_from(payload?.clone()) {
            Ok(conference) => conference,
            Err(e) => {
                log::warn!("bookmark {room} is not valid: {e}");
                return None;
            }
        };
        Some(Self {
            room,
            name: conference.name,
            autojoin: conference.autojoin,
            nick: conference.nick.map(|n| n.as_str().to_owned()),
            password: conference.password,
            extensions: conference.extensions.map(|e| e.payloads),
        })
    }

    fn conference(&self) -> Option<Conference> {
        let mut conference = Conference::new();
        conference.autojoin = self.autojoin;
        conference.name = self.name.clone();
        if let Some(nick) = &self.nick {
            conference.nick = Some(ResourcePart::new(nick).ok()?.into());
        }
        conference.password = self.password.clone();
        conference.extensions = self
            .extensions
            .clone()
            .map(|payloads| Extensions { payloads });
        Some(conference)
    }
}

/// Ask for the bookmarks. The answer starts the autojoin.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    let iq = Iq::from_get("", PubSub::Items(Items::new(NODE_BOOKMARKS)));
    ctx.request(iq, FeaturePending::Bookmarks(Pending::Fetch));
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Fetch => on_fetched(ctx, response),
        Pending::Publish { bookmark, reply } => {
            let result = match response {
                IqResponse::Result(_) => {
                    upsert_row(ctx, &bookmark);
                    muc::mark_room(ctx, &bookmark.room);
                    Ok(())
                }
                IqResponse::Error(e) => Err(ClientError::Server(muc::error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            let _ = reply.send(result);
        }
        Pending::Retract { room, reply } => {
            let result = match response {
                IqResponse::Result(_) => Ok(()),
                // The bookmark is gone already.
                IqResponse::Error(e)
                    if matches!(e.defined_condition, DefinedCondition::ItemNotFound) =>
                {
                    Ok(())
                }
                IqResponse::Error(e) => Err(ClientError::Server(muc::error_text(&e))),
                IqResponse::Lost => Err(ClientError::NotConnected),
            };
            if result.is_ok() {
                unmark(ctx, &room);
                muc::mark_room(ctx, &room);
            }
            let _ = reply.send(result);
        }
    }
}

fn on_fetched(ctx: &mut Ctx<'_>, response: IqResponse) {
    let bookmarks = match response {
        IqResponse::Result(Some(payload)) => match PubSub::try_from(payload) {
            Ok(PubSub::Items(items)) => items
                .items
                .iter()
                .filter_map(|i| Bookmark::parse(i.id.as_ref(), i.payload.as_ref()))
                .collect::<Vec<_>>(),
            _ => {
                log::warn!("the bookmarks answer is not an items list");
                Vec::new()
            }
        },
        IqResponse::Result(None) => Vec::new(),
        // No node yet: there are no bookmarks.
        IqResponse::Error(e) if matches!(e.defined_condition, DefinedCondition::ItemNotFound) => {
            Vec::new()
        }
        IqResponse::Error(e) => {
            // Keep the copy. Join the rooms that it marks.
            log::warn!("cannot fetch bookmarks: {}", muc::error_text(&e));
            for room in rooms_where(ctx, "bookmarked = 1 AND autojoin = 1") {
                muc::join_room(ctx, &room, None, None, None);
            }
            return;
        }
        IqResponse::Lost => return,
    };
    replace_all(ctx, &bookmarks);
    for bookmark in bookmarks.iter().filter(|b| b.autojoin) {
        muc::join_room(ctx, &bookmark.room, None, None, None);
    }
}

/// A PEP event on our bookmarks node: bookmarks were added, changed, or removed.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, payload: Payload) {
    match payload {
        Payload::Items {
            published,
            retracted,
            ..
        } => {
            for item in &published {
                let Some(bookmark) = Bookmark::parse(item.id.as_ref(), item.payload.as_ref())
                else {
                    continue;
                };
                let was_autojoin = autojoin_of(ctx, &bookmark.room);
                upsert(ctx, &bookmark);
                if bookmark.autojoin && was_autojoin != Some(true) {
                    muc::join_room(ctx, &bookmark.room, None, None, None);
                } else if !bookmark.autojoin && was_autojoin == Some(true) {
                    muc::leave_room_quietly(ctx, &bookmark.room);
                }
                muc::mark_room(ctx, &bookmark.room);
            }
            for id in retracted {
                let Ok(room) = BareJid::new(&id.0) else {
                    continue;
                };
                let was_autojoin = autojoin_of(ctx, &room);
                unmark(ctx, &room);
                if was_autojoin == Some(true) {
                    muc::leave_room_quietly(ctx, &room);
                }
                muc::mark_room(ctx, &room);
            }
        }
        // The node is gone or empty: no bookmarks are left.
        Payload::Purge { .. } | Payload::Delete { .. } => {
            let rooms = rooms_where(ctx, "bookmarked = 1 AND autojoin = 1");
            replace_all(ctx, &[]);
            for room in rooms {
                muc::leave_room_quietly(ctx, &room);
            }
        }
        Payload::Configuration { .. } | Payload::Subscription { .. } => {}
    }
}

/// Publish a bookmark. The password of the room, if we have one, goes with it. With no
/// `nick`, the bookmark gets the nick that we use in the room (or asked for), so that the
/// next login joins with it.
pub(crate) fn add(
    ctx: &mut Ctx<'_>,
    room: BareJid,
    name: Option<String>,
    autojoin: bool,
    nick: Option<String>,
    share_password: Option<bool>,
    reply: Reply,
) {
    // The password goes into the bookmark only when the user agreed, or when the bookmark
    // carried it already (another client published it). The bookmark node is private, but
    // the password would then sit on the server in clear text.
    let share = share_password.unwrap_or_else(|| muc::password_shared(ctx, &room));
    let password = if share {
        muc::stored_password(ctx, &room)
    } else {
        None
    };
    // Say it before the publish: our own event comes back, and it must not remove a
    // password that the user chose to keep to himself.
    muc::set_password_shared(ctx, &room, password.is_some());
    let nick = nick.or_else(|| muc::our_nick(ctx, &room));
    let extensions = stored_extensions(ctx, &room);
    let bookmark = Bookmark {
        room,
        name,
        autojoin,
        nick,
        password,
        extensions,
    };
    let Some(conference) = bookmark.conference() else {
        let _ = reply.send(Err(ClientError::Invalid("the nick is not valid".into())));
        return;
    };
    let item = Item {
        id: Some(ItemId(bookmark.room.to_string())),
        publisher: None,
        payload: Some(conference.into()),
    };
    // XEP-0402, section 3: the node keeps its items, and only we can read it.
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
                node: NodeName(NODE_BOOKMARKS.to_owned()),
                items: vec![item],
            },
            publish_options: Some(PublishOptions {
                form: Some(options),
            }),
        },
    );
    ctx.request_publish(
        iq,
        FeaturePending::Bookmarks(Pending::Publish { bookmark, reply }),
    );
}

/// Retract a bookmark.
pub(crate) fn remove(ctx: &mut Ctx<'_>, room: BareJid, reply: Reply) {
    let item = Item {
        id: Some(ItemId(room.to_string())),
        publisher: None,
        payload: None,
    };
    let iq = Iq::from_set(
        "",
        PubSub::Retract(Retract {
            node: NodeName(NODE_BOOKMARKS.to_owned()),
            notify: true,
            items: vec![item],
        }),
    );
    ctx.request(
        iq,
        FeaturePending::Bookmarks(Pending::Retract { room, reply }),
    );
}

/// Store a bookmark that came from the account. Its password goes to `muc`, which keeps it
/// in the keychain when the client has one.
fn upsert(ctx: &Ctx<'_>, bookmark: &Bookmark) {
    upsert_row(ctx, bookmark);
    muc::on_bookmark_password(ctx, &bookmark.room, bookmark.password.as_deref());
}

/// The row of a bookmark, without the password.
fn upsert_row(ctx: &Ctx<'_>, bookmark: &Bookmark) {
    let result = ctx.store.conn().execute(
        "INSERT INTO rooms (account_id, jid, name, nick, autojoin, bookmarked,
                            bookmark_extensions)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
         ON CONFLICT (account_id, jid) DO UPDATE SET
            name = excluded.name, nick = COALESCE(excluded.nick, nick),
            autojoin = excluded.autojoin, bookmarked = 1,
            bookmark_extensions = excluded.bookmark_extensions",
        params![
            ctx.account_id,
            bookmark.room.as_str(),
            bookmark.name,
            bookmark.nick,
            bookmark.autojoin,
            extensions_xml(bookmark.extensions.as_deref()),
        ],
    );
    if let Err(e) = result {
        ctx.store_error("store a bookmark", e);
    }
}

/// The extension elements as one `<extensions/>` element in text, for the database.
fn extensions_xml(payloads: Option<&[Element]>) -> Option<String> {
    let payloads = payloads?;
    let extensions = Element::builder("extensions", NS_BOOKMARKS2)
        .append_all(payloads.iter().cloned())
        .build();
    Some(String::from(&extensions))
}

/// The extension elements that the stored bookmark of `room` has.
fn stored_extensions(ctx: &Ctx<'_>, room: &BareJid) -> Option<Vec<Element>> {
    let xml: Option<String> = ctx
        .store
        .conn()
        .query_row(
            "SELECT bookmark_extensions FROM rooms WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, room.as_str()],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let element: Element = xml?.parse().ok()?;
    Some(element.children().cloned().collect())
}

fn unmark(ctx: &Ctx<'_>, room: &BareJid) {
    let result = ctx.store.conn().execute(
        "UPDATE rooms SET bookmarked = 0, autojoin = 0, password_shared = 0
         WHERE account_id = ?1 AND jid = ?2",
        params![ctx.account_id, room.as_str()],
    );
    if let Err(e) = result {
        ctx.store_error("remove a bookmark", e);
    }
}

/// Make the table match the list: store each bookmark, and unmark the other rooms.
fn replace_all(ctx: &mut Ctx<'_>, bookmarks: &[Bookmark]) {
    for room in rooms_where(ctx, "bookmarked = 1") {
        if !bookmarks.iter().any(|b| b.room == room) {
            unmark(ctx, &room);
        }
    }
    for bookmark in bookmarks {
        upsert(ctx, bookmark);
    }
    ctx.changed(ViewKey::All);
}

fn rooms_where(ctx: &Ctx<'_>, condition: &str) -> Vec<BareJid> {
    let sql = format!("SELECT jid FROM rooms WHERE account_id = ?1 AND {condition}");
    let rows = ctx.store.conn().prepare(&sql).and_then(|mut stmt| {
        stmt.query_map(params![ctx.account_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
    });
    match rows {
        Ok(rows) => rows.iter().filter_map(|j| BareJid::new(j).ok()).collect(),
        Err(e) => {
            ctx.store_error("list rooms", e);
            Vec::new()
        }
    }
}

/// Whether the room has a bookmark.
pub(crate) fn is_bookmarked(ctx: &Ctx<'_>, room: &BareJid) -> bool {
    autojoin_of(ctx, room).is_some()
}

/// The stored autojoin flag of a bookmark, `None` if the room is not bookmarked.
fn autojoin_of(ctx: &Ctx<'_>, room: &BareJid) -> Option<bool> {
    ctx.store
        .conn()
        .query_row(
            "SELECT autojoin FROM rooms WHERE account_id = ?1 AND jid = ?2 AND bookmarked = 1",
            params![ctx.account_id, room.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::Pending as P;
    use crate::features::testing::Harness;
    use xmpp_parsers::pubsub::event::Item as EventItem;
    use xmpp_parsers::stanza::Stanza;
    use xmpp_parsers::stanza_error::{ErrorType, StanzaError};

    const A: &str = "a@rooms.chord.localhost";
    const B: &str = "b@rooms.chord.localhost";
    const ACCOUNT: &str = crate::features::testing::ACCOUNT;

    fn conference(autojoin: bool, nick: &str) -> Element {
        let mut c = Conference::new();
        c.autojoin = autojoin;
        c.name = Some("Room".into());
        c.nick = Some(ResourcePart::new(nick).unwrap().into());
        c.into()
    }

    fn items_result(entries: &[(&str, bool)]) -> Element {
        let mut xml = String::from(
            "<pubsub xmlns='http://jabber.org/protocol/pubsub'><items node='urn:xmpp:bookmarks:1'>",
        );
        for (room, autojoin) in entries {
            xml.push_str(&format!(
                "<item id='{room}'><conference xmlns='urn:xmpp:bookmarks:1' autojoin='{autojoin}' name='N'><nick>al</nick></conference></item>"
            ));
        }
        xml.push_str("</items></pubsub>");
        xml.parse().unwrap()
    }

    fn is_bookmarks(p: &P) -> bool {
        matches!(p, P::Bookmarks(_))
    }

    fn row(h: &Harness, room: &str) -> Option<(bool, bool, Option<String>)> {
        h.store
            .conn()
            .query_row(
                "SELECT bookmarked, autojoin, nick FROM rooms WHERE account_id = ?1 AND jid = ?2",
                params![h.account_id, room],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok()
    }

    fn join_presences(h: &mut Harness) -> Vec<String> {
        h.take_sent()
            .into_iter()
            .filter_map(|s| match s {
                Stanza::Presence(p) => p.to.map(|t| t.to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn fetch_stores_bookmarks_removes_old_ones_and_autojoins() {
        let mut h = Harness::new();
        // An old bookmark that the server no longer has.
        h.with_ctx(|ctx| {
            upsert(
                ctx,
                &Bookmark {
                    room: BareJid::new(B).unwrap(),
                    name: None,
                    autojoin: true,
                    nick: None,
                    password: None,
                    extensions: None,
                },
            )
        });
        h.with_ctx(on_connected);
        let iqs = h.sent_iqs();
        assert!(
            matches!(&iqs[0], Iq::Get { payload, .. } if payload.is("pubsub", "http://jabber.org/protocol/pubsub"))
        );
        assert!(iqs[0].to().is_none(), "sent to our own account");
        h.answer(is_bookmarks, Some(items_result(&[(A, true)])));
        assert_eq!(row(&h, A), Some((true, true, Some("al".into()))));
        assert_eq!(row(&h, B), Some((false, false, None)));
        assert_eq!(join_presences(&mut h), vec![format!("{A}/al")]);
        assert!(h.take_dirty().contains(&ViewKey::All));
    }

    #[test]
    fn fetch_without_autojoin_does_not_join_and_missing_node_clears() {
        let mut h = Harness::new();
        h.with_ctx(on_connected);
        h.answer(is_bookmarks, Some(items_result(&[(A, false)])));
        assert!(join_presences(&mut h).is_empty());
        h.with_ctx(on_connected);
        let error = StanzaError::new(ErrorType::Cancel, DefinedCondition::ItemNotFound, "en", "");
        h.respond(is_bookmarks, IqResponse::Error(error));
        assert!(!row(&h, A).unwrap().0);
    }

    #[test]
    fn fetch_error_joins_the_stored_autojoin_rooms_and_lost_does_nothing() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            upsert(
                ctx,
                &Bookmark {
                    room: BareJid::new(A).unwrap(),
                    name: None,
                    autojoin: true,
                    nick: Some("al".into()),
                    password: None,
                    extensions: None,
                },
            )
        });
        h.with_ctx(on_connected);
        h.respond(is_bookmarks, IqResponse::Lost);
        assert!(join_presences(&mut h).is_empty());
        h.with_ctx(on_connected);
        let error = StanzaError::new(
            ErrorType::Wait,
            DefinedCondition::InternalServerError,
            "en",
            "",
        );
        h.respond(is_bookmarks, IqResponse::Error(error));
        assert_eq!(row(&h, A), Some((true, true, Some("al".into()))));
        assert_eq!(join_presences(&mut h), vec![format!("{A}/al")]);
    }

    fn event(published: Vec<(&str, Element)>, retracted: Vec<&str>) -> Payload {
        Payload::Items {
            node: NodeName(NODE_BOOKMARKS.into()),
            published: published
                .into_iter()
                .map(|(id, payload)| EventItem {
                    id: Some(ItemId(id.into())),
                    publisher: None,
                    payload: Some(payload),
                })
                .collect(),
            retracted: retracted.into_iter().map(|i| ItemId(i.into())).collect(),
        }
    }

    #[test]
    fn events_keep_the_table_in_sync_and_join_or_leave() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference(true, "al"))], vec![])));
        assert_eq!(row(&h, A), Some((true, true, Some("al".into()))));
        assert_eq!(join_presences(&mut h), vec![format!("{A}/al")]);

        // Pretend that the join finished, then turn autojoin off: we leave.
        h.with_ctx(|ctx| {
            ctx.state.muc.joins.clear();
            ctx.state
                .muc
                .nicks
                .insert(BareJid::new(A).unwrap(), "al".into());
        });
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference(false, "al"))], vec![])));
        let sent = h.take_sent();
        assert!(
            matches!(&sent[..], [Stanza::Presence(p)] if p.type_ == xmpp_parsers::presence::Type::Unavailable),
            "{sent:?}"
        );
        assert_eq!(row(&h, A), Some((true, false, Some("al".into()))));

        h.with_ctx(|ctx| on_event(ctx, event(vec![], vec![A])));
        assert!(!row(&h, A).unwrap().0);
        // An invalid item is skipped.
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                event(vec![("not a jid@", conference(true, "al"))], vec![]),
            )
        });
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn purge_removes_all_bookmarks() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference(false, "al"))], vec![])));
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                Payload::Purge {
                    node: NodeName(NODE_BOOKMARKS.into()),
                },
            )
        });
        assert!(!row(&h, A).unwrap().0);
    }

    #[test]
    fn add_publishes_with_options_and_stores_on_success() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            add(
                ctx,
                BareJid::new(A).unwrap(),
                Some("Room".into()),
                true,
                Some("al".into()),
                None,
                reply,
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set")
        };
        let PubSub::Publish {
            publish,
            publish_options,
        } = PubSub::try_from(payload.clone()).unwrap()
        else {
            panic!("not a publish")
        };
        assert_eq!(publish.node.0, NODE_BOOKMARKS);
        assert_eq!(publish.items[0].id, Some(ItemId(A.into())));
        let form = publish_options.unwrap().form.unwrap();
        assert_eq!(
            form.form_type(),
            Some("http://jabber.org/protocol/pubsub#publish-options")
        );
        let value = |var: &str| {
            form.fields
                .iter()
                .find(|f| f.var.as_deref() == Some(var))
                .unwrap()
                .values
                .clone()
        };
        assert_eq!(value("pubsub#persist_items"), vec!["true"]);
        assert_eq!(value("pubsub#max_items"), vec!["max"]);
        assert_eq!(value("pubsub#send_last_published_item"), vec!["never"]);
        assert_eq!(value("pubsub#access_model"), vec!["whitelist"]);
        assert!(answer.try_recv().unwrap().is_none(), "waits for the server");
        assert_eq!(row(&h, A), None);

        h.answer(is_bookmarks, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(row(&h, A), Some((true, true, Some("al".into()))));
        // A local change does not join.
        assert!(join_presences(&mut h).is_empty());
    }

    #[test]
    fn add_without_a_nick_uses_the_nick_of_the_room() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            muc::join_room(
                ctx,
                &BareJid::new(A).unwrap(),
                Some("Reserved".into()),
                None,
                None,
            )
        });
        h.take_sent();
        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| add(ctx, BareJid::new(A).unwrap(), None, true, None, None, reply));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set")
        };
        let PubSub::Publish { publish, .. } = PubSub::try_from(payload.clone()).unwrap() else {
            panic!("not a publish")
        };
        let conference = Conference::try_from(publish.items[0].payload.clone().unwrap()).unwrap();
        assert_eq!(
            conference.nick.map(|n| n.as_str().to_owned()),
            Some("Reserved".into())
        );
    }

    #[test]
    fn add_error_and_lost_answer_the_command() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            add(
                ctx,
                BareJid::new(A).unwrap(),
                None,
                false,
                None,
                None,
                reply,
            )
        });
        let error = StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::Forbidden,
            "en",
            "no access",
        );
        h.respond(is_bookmarks, IqResponse::Error(error));
        let Some(Err(ClientError::Server(text))) = answer.try_recv().unwrap() else {
            panic!("no server error")
        };
        assert!(text.contains("forbidden"), "{text}");
        assert_eq!(row(&h, A), None);

        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            add(
                ctx,
                BareJid::new(A).unwrap(),
                None,
                false,
                None,
                None,
                reply,
            )
        });
        h.respond(is_bookmarks, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );

        // A nick that resourceprep refuses.
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            add(
                ctx,
                BareJid::new(A).unwrap(),
                None,
                false,
                Some(String::new()),
                None,
                reply,
            )
        });
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
    }

    #[test]
    fn remove_retracts_and_clears_the_flags() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference(false, "al"))], vec![])));
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| remove(ctx, BareJid::new(A).unwrap(), reply));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set")
        };
        let PubSub::Retract(retract) = PubSub::try_from(payload.clone()).unwrap() else {
            panic!("not a retract")
        };
        assert_eq!(retract.items[0].id, Some(ItemId(A.into())));
        assert!(retract.notify);
        h.answer(is_bookmarks, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(!row(&h, A).unwrap().0);

        // item-not-found counts as removed. Other errors do not.
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| remove(ctx, BareJid::new(B).unwrap(), reply));
        let error = StanzaError::new(ErrorType::Cancel, DefinedCondition::ItemNotFound, "en", "");
        h.respond(is_bookmarks, IqResponse::Error(error));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| remove(ctx, BareJid::new(B).unwrap(), reply));
        let error = StanzaError::new(ErrorType::Auth, DefinedCondition::Forbidden, "en", "");
        h.respond(is_bookmarks, IqResponse::Error(error));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| remove(ctx, BareJid::new(B).unwrap(), reply));
        h.respond(is_bookmarks, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn leave_retracts_the_bookmark_and_a_room_without_one_sends_no_iq() {
        use crate::features::muc::Command;
        let mut h = Harness::new();
        // A bookmarked room that we joined.
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference(true, "al"))], vec![])));
        h.take_sent();
        h.with_ctx(|ctx| {
            ctx.state.muc.joins.clear();
            ctx.state
                .muc
                .nicks
                .insert(BareJid::new(A).unwrap(), "al".into());
        });
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            muc::on_command(
                ctx,
                Command::Leave {
                    room: BareJid::new(A).unwrap(),
                    reply,
                },
            )
        });
        let sent = h.take_sent();
        assert!(
            sent.iter().any(|s| matches!(s, Stanza::Presence(p) if p.type_ == xmpp_parsers::presence::Type::Unavailable)),
            "{sent:?}"
        );
        let retracts = sent
            .iter()
            .filter(|s| matches!(s, Stanza::Iq(Iq::Set { payload, .. }) if payload.is("pubsub", "http://jabber.org/protocol/pubsub")))
            .count();
        assert_eq!(retracts, 1, "{sent:?}");
        assert!(answer.try_recv().unwrap().is_none(), "waits for the server");
        h.answer(is_bookmarks, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(row(&h, A), Some((false, false, Some("al".into()))));

        // A room in a space has no bookmark: leaving it sends no pubsub IQ.
        h.with_ctx(|ctx| {
            ctx.store
                .conn()
                .execute(
                    "INSERT INTO rooms (account_id, jid, nick, joined) VALUES (?1, ?2, 'al', 1)",
                    params![ctx.account_id, B],
                )
                .unwrap();
        });
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            muc::on_command(
                ctx,
                Command::Leave {
                    room: BareJid::new(B).unwrap(),
                    reply,
                },
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(h.sent_iqs().is_empty());
    }

    fn conference_xml(inner: &str) -> Element {
        format!(
            "<conference xmlns='urn:xmpp:bookmarks:1' autojoin='false' name='N'><nick>al</nick>{inner}</conference>"
        )
        .parse()
        .unwrap()
    }

    /// The conference element of the last publish that the harness sent.
    fn published(h: &mut Harness) -> Conference {
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set")
        };
        let PubSub::Publish { publish, .. } = PubSub::try_from(payload.clone()).unwrap() else {
            panic!("not a publish")
        };
        Conference::try_from(publish.items[0].payload.clone().unwrap()).unwrap()
    }

    fn republish(h: &mut Harness, share: Option<bool>) -> Conference {
        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            add(
                ctx,
                BareJid::new(A).unwrap(),
                None,
                false,
                None,
                share,
                reply,
            )
        });
        published(h)
    }

    fn column_password(h: &Harness) -> Option<String> {
        h.store
            .conn()
            .query_row(
                "SELECT password FROM rooms WHERE account_id = ?1 AND jid = ?2",
                params![h.account_id, A],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn a_republish_keeps_the_extensions_that_another_client_wrote() {
        let mut h = Harness::new();
        let with = conference_xml(
            "<extensions><state xmlns='urn:example:state' read='5'/><other xmlns='urn:example:o'>x</other></extensions>",
        );
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, with)], vec![])));
        let conference = republish(&mut h, None);
        let extensions = conference.extensions.expect("the extensions stay");
        assert_eq!(extensions.payloads.len(), 2);
        assert!(extensions.payloads[0].is("state", "urn:example:state"));
        assert_eq!(extensions.payloads[0].attr("read"), Some("5"));
        assert!(extensions.payloads[1].is("other", "urn:example:o"));
        assert_eq!(extensions.payloads[1].text(), "x");
    }

    #[test]
    fn an_empty_extensions_element_stays_and_no_element_stays_absent() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                event(vec![(A, conference_xml("<extensions/>"))], vec![]),
            )
        });
        let conference = republish(&mut h, None);
        assert!(conference.extensions.is_some_and(|e| e.payloads.is_empty()));

        let mut h = Harness::new();
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference_xml(""))], vec![])));
        assert!(republish(&mut h, None).extensions.is_none());
    }

    #[test]
    fn a_fetched_bookmark_parses_its_extensions() {
        let payload = conference_xml("<extensions><x xmlns='urn:example:x'/></extensions>");
        let bookmark = Bookmark::parse(Some(&ItemId(A.into())), Some(&payload)).unwrap();
        let extensions = bookmark.extensions.clone().expect("extensions");
        assert_eq!(extensions.len(), 1);
        let again = bookmark.conference().unwrap();
        assert_eq!(again.extensions.unwrap().payloads.len(), 1);
    }

    #[test]
    fn the_extensions_of_a_bookmark_that_the_user_changes_in_place_are_not_lost() {
        let mut h = Harness::new();
        let with = conference_xml("<extensions><k xmlns='urn:example:k'/></extensions>");
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, with)], vec![])));
        // Publish twice, with the answer of the server in between.
        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| add(ctx, BareJid::new(A).unwrap(), None, true, None, None, reply));
        published(&mut h);
        h.answer(is_bookmarks, None);
        let conference = republish(&mut h, None);
        assert_eq!(conference.extensions.unwrap().payloads.len(), 1);
    }

    #[test]
    fn a_bookmark_carries_the_password_only_when_the_user_agrees() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            muc::join_room(
                ctx,
                &BareJid::new(A).unwrap(),
                Some("al".into()),
                Some("hunter2".into()),
                None,
            )
        });
        h.take_sent();
        // No choice, and the bookmark never had it: no password.
        assert_eq!(republish(&mut h, None).password, None);
        // The user agrees.
        assert_eq!(
            republish(&mut h, Some(true)).password.as_deref(),
            Some("hunter2")
        );
        // The bookmark had it, so a republish with no choice keeps it.
        assert_eq!(republish(&mut h, None).password.as_deref(), Some("hunter2"));
        // The user withdraws. The password stays on this device.
        assert_eq!(republish(&mut h, Some(false)).password, None);
        assert_eq!(column_password(&h).as_deref(), Some("hunter2"));
        assert_eq!(republish(&mut h, None).password, None);
    }

    #[test]
    fn the_echo_of_our_own_bookmark_without_the_password_keeps_the_local_password() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            muc::join_room(
                ctx,
                &BareJid::new(A).unwrap(),
                Some("al".into()),
                Some("hunter2".into()),
                None,
            )
        });
        republish(&mut h, Some(true));
        republish(&mut h, Some(false));
        // The event that our publish causes comes back with no password.
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference_xml(""))], vec![])));
        assert_eq!(column_password(&h).as_deref(), Some("hunter2"));
    }

    #[test]
    fn a_password_that_another_client_published_is_kept_and_published_again() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                event(
                    vec![(A, conference_xml("<password>from-phone</password>"))],
                    vec![],
                ),
            )
        });
        assert_eq!(column_password(&h).as_deref(), Some("from-phone"));
        assert_eq!(
            republish(&mut h, None).password.as_deref(),
            Some("from-phone")
        );
        // The other client removes the password from the bookmark: it goes from here too.
        h.with_ctx(|ctx| on_event(ctx, event(vec![(A, conference_xml(""))], vec![])));
        assert_eq!(column_password(&h), None);
    }

    #[test]
    fn a_password_from_a_bookmark_goes_to_the_keychain() {
        use crate::secrets::testing::MemorySecrets;
        let mut h = Harness::new();
        let secrets = std::sync::Arc::new(MemorySecrets::default());
        h.state.muc.set_secret_store(secrets.clone());
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                event(
                    vec![(A, conference_xml("<password>from-phone</password>"))],
                    vec![],
                ),
            )
        });
        assert_eq!(column_password(&h).as_deref(), Some(""), "a marker only");
        assert_eq!(
            secrets
                .map
                .lock()
                .unwrap()
                .get(&crate::secrets::room_password_key(ACCOUNT, A))
                .map(String::as_str),
            Some("from-phone")
        );
        // And the republish reads it from there.
        assert_eq!(
            republish(&mut h, None).password.as_deref(),
            Some("from-phone")
        );
    }

    #[test]
    fn a_removed_bookmark_shares_nothing_when_the_room_is_bookmarked_again() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| {
            on_event(
                ctx,
                event(
                    vec![(A, conference_xml("<password>from-phone</password>"))],
                    vec![],
                ),
            )
        });
        assert_eq!(
            republish(&mut h, None).password.as_deref(),
            Some("from-phone")
        );
        // The bookmark goes (we left the room, or another client removed it).
        h.with_ctx(|ctx| on_event(ctx, event(vec![], vec![A])));
        assert_eq!(
            column_password(&h).as_deref(),
            Some("from-phone"),
            "kept here"
        );
        // A new bookmark of the room has no password until the user agrees.
        assert_eq!(republish(&mut h, None).password, None);
        assert_eq!(
            republish(&mut h, Some(true)).password.as_deref(),
            Some("from-phone")
        );
    }
}
