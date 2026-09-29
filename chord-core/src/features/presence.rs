//! Presence (RFC 6121): our own presence. Contact presence is in `roster`, room presence
//! in `muc`.
//!
//! Our show value (away, dnd, xa) and status text live in the `own_presence` table, so
//! every presence and room join carries them, also after a restart.
//!
//! Invisible (XEP-0126): Chord stays available to the server, so messages arrive. An
//! active privacy list (XEP-0016) stops our presence to the contacts that see it. Rooms
//! still get our presence: a room occupant must send presence.

use futures_channel::oneshot;
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::presence::{Presence, Show, Type as PresenceType};

use super::roster::Subscription;
use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, avatars, disco};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::store::Store;

/// Our availability, as the frontends choose it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum Availability {
    #[default]
    Available,
    Away,
    /// Do not disturb.
    Dnd,
    /// Away for a longer time.
    ExtendedAway,
    /// Contacts see us as offline. Needs privacy lists (XEP-0016) on the server.
    Invisible,
}

impl Availability {
    fn show(self) -> Option<Show> {
        match self {
            Self::Available | Self::Invisible => None,
            Self::Away => Some(Show::Away),
            Self::Dnd => Some(Show::Dnd),
            Self::ExtendedAway => Some(Show::Xa),
        }
    }

    fn as_str(self) -> Option<&'static str> {
        match self {
            Self::Available => None,
            Self::Away => Some("away"),
            Self::Dnd => Some("dnd"),
            Self::ExtendedAway => Some("xa"),
            Self::Invisible => Some("invisible"),
        }
    }

    fn parse(text: Option<&str>) -> Self {
        match text {
            Some("away") => Self::Away,
            Some("dnd") => Self::Dnd,
            Some("xa") => Self::ExtendedAway,
            Some("invisible") => Self::Invisible,
            _ => Self::Available,
        }
    }
}

/// Our own presence: the availability and an optional status text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct OwnPresence {
    pub availability: Availability,
    pub status: Option<String>,
}

/// The longest status text that Chord sends.
const MAX_STATUS_CHARS: usize = 128;

/// The namespace of privacy lists (XEP-0016).
const NS_PRIVACY: &str = "jabber:iq:privacy";

/// The name of the privacy list that makes us invisible.
const INVISIBLE_LIST: &str = "chord-invisible";

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The store of the invisible list. Only an error needs an action.
    StoreList,
    /// The activation of the invisible list. `reply` is `None` at connect.
    Hide { reply: Option<Reply<()>> },
    /// The deactivation of the invisible list.
    Show {
        previous: Availability,
        reply: Reply<()>,
    },
}

/// A command from the public API.
pub(crate) enum Command {
    Set {
        presence: OwnPresence,
        reply: Reply<()>,
    },
    Get {
        reply: Reply<OwnPresence>,
    },
}

impl ClientHandle {
    /// Set our availability and status text. Chord sends the new presence to the server
    /// and to every room that we are in, and keeps it for the next sessions. Offline, it
    /// only stores it. A status text over 128 characters is cut.
    pub async fn set_presence(
        &self,
        availability: Availability,
        status: Option<String>,
    ) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        let presence = OwnPresence {
            availability,
            status,
        };
        self.feature(FeatureCommand::Presence(Command::Set { presence, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Our stored availability and status text. Works offline.
    pub async fn own_presence(&self) -> Result<OwnPresence, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Presence(Command::Get { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// Send initial presence (RFC 6121, 4.2) with our entity capabilities (XEP-0115). The
/// server routes chat messages to a resource only after it is available. PEP sends
/// notifications (`+notify`) only to clients whose caps ask for them.
///
/// It also carries our show value and status text, and, if the store has our avatar,
/// its hash (XEP-0153), so that vCard clients see it.
///
/// Invisible, it first activates the invisible list, and sends the presence after the
/// answer. The server handles our stanzas in order, so no presence goes out before it.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    if own(ctx).availability == Availability::Invisible {
        hide(ctx, None);
        return;
    }
    let presence = current(ctx);
    ctx.send(presence);
}

fn own(ctx: &mut Ctx<'_>) -> OwnPresence {
    load(ctx.store, ctx.account_id).unwrap_or_else(|e| {
        ctx.store_error("read our presence", e);
        OwnPresence::default()
    })
}

/// Store the invisible list and make it active. The list stops presence to every contact
/// that has a subscription to it.
fn hide(ctx: &mut Ctx<'_>, reply: Option<Reply<()>>) {
    let store = format!(
        "<query xmlns='{NS_PRIVACY}'><list name='{INVISIBLE_LIST}'>\
         <item type='subscription' value='both' action='deny' order='1'><presence-out/></item>\
         <item type='subscription' value='from' action='deny' order='2'><presence-out/></item>\
         </list></query>"
    );
    ctx.request(set(&store), FeaturePending::Presence(Pending::StoreList));
    let activate = format!("<query xmlns='{NS_PRIVACY}'><active name='{INVISIBLE_LIST}'/></query>");
    ctx.request(
        set(&activate),
        FeaturePending::Presence(Pending::Hide { reply }),
    );
}

/// Make no privacy list active, so that the contacts see our presence again.
fn unhide(ctx: &mut Ctx<'_>, previous: Availability, reply: Reply<()>) {
    let decline = format!("<query xmlns='{NS_PRIVACY}'><active/></query>");
    ctx.request(
        set(&decline),
        FeaturePending::Presence(Pending::Show { previous, reply }),
    );
}

/// A privacy list IQ to our own account. xmpp-parsers 0.23 has no XEP-0016 types.
fn set(payload: &str) -> Iq {
    Iq::Set {
        from: None,
        to: None,
        id: String::new(),
        payload: payload.parse::<Element>().expect("a valid privacy query"),
    }
}

/// Send our presence to the server, which sends it to the contacts, and to the rooms.
fn broadcast(ctx: &mut Ctx<'_>) {
    let presence = current(ctx);
    ctx.send(presence.clone());
    super::muc::send_presence_to_rooms(ctx, &presence);
}

/// Tell each contact that sees our presence that we went offline. After this, the
/// invisible list stops our presence to them.
fn send_unavailable_to_contacts(ctx: &mut Ctx<'_>) {
    let contacts = match super::roster::list_contacts(ctx.store.conn(), ctx.account_id) {
        Ok(contacts) => contacts,
        Err(e) => {
            ctx.store_error("read the contacts", e);
            return;
        }
    };
    for contact in contacts {
        if matches!(
            contact.subscription,
            Subscription::From | Subscription::Both
        ) {
            let presence = Presence::new(PresenceType::Unavailable).with_to(contact.jid);
            ctx.send(presence);
        }
    }
}

fn set_availability(ctx: &mut Ctx<'_>, availability: Availability) {
    let mut presence = own(ctx);
    presence.availability = availability;
    if let Err(e) = save(ctx.store, ctx.account_id, &presence) {
        ctx.store_error("store our presence", e);
    }
}

fn describe(response: &IqResponse) -> String {
    match response {
        IqResponse::Error(error) => {
            let text = error.texts.values().next().cloned();
            text.unwrap_or_else(|| format!("{:?}", error.defined_condition))
        }
        _ => "the session closed".to_owned(),
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let ok = matches!(response, IqResponse::Result(_));
    match pending {
        // The activation after it fails too, and reports the error.
        Pending::StoreList => {}
        Pending::Hide { reply } => {
            if !ok && !matches!(response, IqResponse::Lost) {
                // The server cannot hide us. Show us as available, and say why.
                set_availability(ctx, Availability::Available);
                ctx.emit(ClientEvent::Notice(
                    "This server cannot make you invisible. You appear available.".to_owned(),
                ));
            }
            if !matches!(response, IqResponse::Lost) {
                broadcast(ctx);
            }
            if let Some(reply) = reply {
                let result = if ok {
                    Ok(())
                } else {
                    Err(ClientError::Invalid(format!(
                        "invisible: {}",
                        describe(&response)
                    )))
                };
                let _ = reply.send(result);
            }
        }
        Pending::Show { previous, reply } => {
            if ok {
                broadcast(ctx);
                let _ = reply.send(Ok(()));
            } else {
                set_availability(ctx, previous);
                let _ = reply.send(Err(ClientError::Invalid(format!(
                    "visible: {}",
                    describe(&response)
                ))));
            }
        }
    }
}

/// Available presence with our caps.
pub fn initial() -> Presence {
    Presence::available().with_payloads(vec![disco::caps().into()])
}

/// Our presence now: caps, the avatar hash if any, the show value and the status text.
/// The room join uses it too.
pub(crate) fn current(ctx: &mut Ctx<'_>) -> Presence {
    let mut presence = match avatars::load(ctx.store, ctx.account_id, ctx.account) {
        Ok(Some(avatar)) => avatars::vcard_presence(Some(&avatar.hash)),
        Ok(None) => initial(),
        Err(e) => {
            ctx.store_error("read our avatar", e);
            initial()
        }
    };
    let own = own(ctx);
    presence.show = own.availability.show();
    if let Some(status) = own.status.filter(|s| !s.is_empty()) {
        presence.set_status("", status);
    }
    presence
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Set { presence, reply } => {
            let previous = own(ctx).availability;
            if let Err(e) = save(ctx.store, ctx.account_id, &presence) {
                let _ = reply.send(Err(ClientError::Invalid(format!("store: {e}"))));
                return;
            }
            let invisible = Availability::Invisible;
            match (previous == invisible, presence.availability == invisible) {
                (false, true) => {
                    send_unavailable_to_contacts(ctx);
                    hide(ctx, Some(reply));
                }
                (true, false) => unhide(ctx, previous, reply),
                _ => {
                    broadcast(ctx);
                    let _ = reply.send(Ok(()));
                }
            }
        }
        Command::Get { reply } => {
            let _ = reply.send(
                load(ctx.store, ctx.account_id)
                    .map_err(|e| ClientError::Invalid(format!("store: {e}"))),
            );
        }
    }
}

/// A command while no session is up: store and read work.
pub(crate) fn offline(store: &Store, account_id: i64, command: Command) {
    match command {
        Command::Set { presence, reply } => {
            let _ = reply.send(
                save(store, account_id, &presence)
                    .map_err(|e| ClientError::Invalid(format!("store: {e}"))),
            );
        }
        Command::Get { reply } => {
            let _ = reply.send(
                load(store, account_id).map_err(|e| ClientError::Invalid(format!("store: {e}"))),
            );
        }
    }
}

fn save(store: &Store, account_id: i64, presence: &OwnPresence) -> rusqlite::Result<()> {
    let status: Option<String> = presence
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(MAX_STATUS_CHARS).collect());
    store.conn().execute(
        "INSERT INTO own_presence (account_id, show, status) VALUES (?1, ?2, ?3)
         ON CONFLICT (account_id) DO UPDATE SET show = excluded.show, status = excluded.status",
        params![account_id, presence.availability.as_str(), status],
    )?;
    Ok(())
}

pub(crate) fn load(store: &Store, account_id: i64) -> rusqlite::Result<OwnPresence> {
    let row: Option<(Option<String>, Option<String>)> = store
        .conn()
        .query_row(
            "SELECT show, status FROM own_presence WHERE account_id = ?1",
            params![account_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row
        .map(|(show, status)| OwnPresence {
            availability: Availability::parse(show.as_deref()),
            status,
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::stanza::Stanza;
    use xmpp_parsers::vcard_update::VCardUpdate;

    use super::*;
    use crate::features::testing::Harness;

    fn update(stanza: &Stanza) -> Option<VCardUpdate> {
        let Stanza::Presence(p) = stanza else {
            panic!("expected a presence: {stanza:?}");
        };
        p.payloads
            .iter()
            .find_map(|e| VCardUpdate::try_from(e.clone()).ok())
    }

    #[test]
    fn initial_presence_carries_our_avatar_hash() {
        let mut h = Harness::new();
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        assert!(update(&sent[0]).is_none(), "no avatar, no update element");

        let hash = "0123456789abcdef0123456789abcdef01234567";
        h.store
            .conn()
            .execute(
                "INSERT INTO avatars (account_id, owner, hash, mime, data)
                 VALUES (?1, ?2, ?3, 'image/png', x'00')",
                rusqlite::params![h.account_id, h.account.as_str(), hash],
            )
            .unwrap();
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        let photo = update(&sent[0]).and_then(|u| u.photo).and_then(|p| p.data);
        assert_eq!(photo.map(|d| d[0]), Some(0x01));
    }

    #[test]
    fn our_show_and_status_go_out_and_to_the_rooms() {
        use crate::features::{FeatureCommand, on_command};
        let mut h = Harness::new();
        h.state.muc.nicks.insert(
            jid::BareJid::new("dev@rooms.chord.localhost").unwrap(),
            "alice".into(),
        );
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Presence(Command::Set {
                    presence: OwnPresence {
                        availability: Availability::Dnd,
                        status: Some("  On the range until 17:00  ".into()),
                    },
                    reply,
                }),
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        let presences: Vec<&Presence> = sent
            .iter()
            .filter_map(|s| match s {
                Stanza::Presence(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(presences.len(), 2, "one broadcast, one to the room");
        assert!(presences.iter().all(|p| p.show == Some(Show::Dnd)));
        assert_eq!(
            presences[0].statuses.get("").map(String::as_str),
            Some("On the range until 17:00")
        );
        assert_eq!(
            presences[1].to.as_ref().map(|j| j.as_str()),
            Some("dev@rooms.chord.localhost/alice")
        );

        // The next session sends it again in the initial presence.
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        let Stanza::Presence(p) = &sent[0] else {
            panic!("expected a presence")
        };
        assert_eq!(p.show, Some(Show::Dnd));
    }

    #[test]
    fn presence_is_stored_offline_and_available_clears_the_show() {
        let h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        offline(
            &h.store,
            h.account_id,
            Command::Set {
                presence: OwnPresence {
                    availability: Availability::Away,
                    status: Some("x".repeat(200)),
                },
                reply,
            },
        );
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let own = load(&h.store, h.account_id).unwrap();
        assert_eq!(own.availability, Availability::Away);
        assert_eq!(own.status.map(|s| s.chars().count()), Some(128));

        save(&h.store, h.account_id, &OwnPresence::default()).unwrap();
        assert_eq!(
            load(&h.store, h.account_id).unwrap(),
            OwnPresence::default()
        );
    }

    fn set_command(
        h: &mut Harness,
        availability: Availability,
    ) -> oneshot::Receiver<Result<(), ClientError>> {
        use crate::features::{FeatureCommand, on_command};
        let (reply, answer) = oneshot::channel();
        let presence = OwnPresence {
            availability,
            status: None,
        };
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Presence(Command::Set { presence, reply }),
            )
        });
        answer
    }

    fn presences(sent: &[Stanza]) -> Vec<&Presence> {
        sent.iter()
            .filter_map(|s| match s {
                Stanza::Presence(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    fn privacy_queries(sent: &[Stanza]) -> Vec<Element> {
        sent.iter()
            .filter_map(|s| match s {
                Stanza::Iq(Iq::Set { payload, .. }) if payload.is("query", NS_PRIVACY) => {
                    Some(payload.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn is_hide(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Presence(Pending::Hide { .. }))
    }

    #[test]
    fn going_invisible_hides_us_from_the_contacts_that_see_us() {
        let mut h = Harness::new();
        for (jid, subscription) in [
            ("both@example.org", "both"),
            ("to@example.org", "to"),
            ("from@example.org", "from"),
        ] {
            h.store
                .conn()
                .execute(
                    "INSERT INTO contacts (account_id, jid, subscription) VALUES (?1, ?2, ?3)",
                    params![h.account_id, jid, subscription],
                )
                .unwrap();
        }
        let mut answer = set_command(&mut h, Availability::Invisible);
        let sent = h.take_sent();
        let gone: Vec<String> = presences(&sent)
            .iter()
            .map(|p| {
                assert_eq!(p.type_, PresenceType::Unavailable);
                p.to.as_ref().unwrap().to_string()
            })
            .collect();
        assert_eq!(gone, ["both@example.org", "from@example.org"]);
        let queries = privacy_queries(&sent);
        assert_eq!(queries.len(), 2, "store the list, then make it active");
        let list = queries[0].get_child("list", NS_PRIVACY).unwrap();
        assert_eq!(list.children().count(), 2);
        let active = queries[1].get_child("active", NS_PRIVACY).unwrap();
        assert_eq!(active.attr("name"), Some(INVISIBLE_LIST));
        assert_eq!(answer.try_recv().unwrap(), None, "waits for the server");

        h.respond(is_hide, IqResponse::Result(None));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        let out = presences(&sent);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].type_, PresenceType::None);
        assert_eq!(out[0].to, None);
        assert_eq!(out[0].show, None);
    }

    #[test]
    fn an_invisible_session_starts_hidden_or_says_why_not() {
        let mut h = Harness::new();
        let invisible = OwnPresence {
            availability: Availability::Invisible,
            status: None,
        };
        save(&h.store, h.account_id, &invisible).unwrap();
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        assert!(
            presences(&sent).is_empty(),
            "no presence before the list is active"
        );
        assert_eq!(privacy_queries(&sent).len(), 2);

        // A server without privacy lists: Chord shows us as available and says why.
        let error = xmpp_parsers::stanza_error::StanzaError::new(
            xmpp_parsers::stanza_error::ErrorType::Cancel,
            xmpp_parsers::stanza_error::DefinedCondition::ServiceUnavailable,
            "en",
            "",
        );
        h.respond(is_hide, IqResponse::Error(error));
        assert_eq!(presences(&h.take_sent()).len(), 1);
        assert_eq!(
            load(&h.store, h.account_id).unwrap().availability,
            Availability::Available
        );
        assert!(
            h.effects
                .iter()
                .any(|e| matches!(e, crate::features::Effect::Emit(ClientEvent::Notice(_))))
        );
    }

    #[test]
    fn leaving_invisible_deactivates_the_list_first() {
        let mut h = Harness::new();
        let invisible = OwnPresence {
            availability: Availability::Invisible,
            status: None,
        };
        save(&h.store, h.account_id, &invisible).unwrap();
        let mut answer = set_command(&mut h, Availability::Away);
        let sent = h.take_sent();
        assert!(presences(&sent).is_empty());
        let queries = privacy_queries(&sent);
        assert_eq!(queries.len(), 1);
        let active = queries[0].get_child("active", NS_PRIVACY).unwrap();
        assert_eq!(active.attr("name"), None, "no list is active");

        h.respond(
            |p| matches!(p, FeaturePending::Presence(Pending::Show { .. })),
            IqResponse::Result(None),
        );
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        assert_eq!(presences(&sent)[0].show, Some(Show::Away));
    }
}
