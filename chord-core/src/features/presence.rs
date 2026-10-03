//! Presence (RFC 6121): our own presence. Contact presence is in `roster`, room presence
//! in `muc`.
//!
//! Our show value (away, dnd, xa) and status text live in the `own_presence` table, so
//! every presence and room join carries them, also after a restart.
//!
//! Invisible: Chord stays available to the server, so messages arrive. Chord picks the
//! mechanism from service discovery, in this order:
//! 1. Invisible command (XEP-0186, `urn:xmpp:invisible:0`): the server does the work.
//! 2. An active privacy list (XEP-0016, `jabber:iq:privacy`, with the XEP-0126 lists)
//!    that stops our presence to the contacts that see it.
//! 3. Neither: Chord says so and keeps us available. No IQ goes out.
//!
//! The privacy list is active for this session only (XEP-0016, 2.3). It is never the default
//! list, so a second resource of the account stays visible. The unavailable presence of
//! this resource goes to each contact as a directed presence: the contact drops this
//! resource and keeps the others.
//!
//! Rooms still get our presence: a room occupant must send presence. Chord knows the
//! mechanism only after service discovery, so an invisible session sends its first
//! presence after discovery.
//!
//! Idle (XEP-0319): `set_idle` puts `<idle since=.../>` into every presence that we send,
//! until the frontend clears it.

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
    /// Contacts see us as offline. Needs the invisible command (XEP-0186) or privacy
    /// lists (XEP-0016) on the server.
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

/// The namespace of the invisible command (XEP-0186).
const NS_INVISIBLE: &str = "urn:xmpp:invisible:0";

/// The namespace of last user interaction in presence (XEP-0319).
pub(crate) const NS_IDLE: &str = "urn:xmpp:idle:1";

/// How the server can hide us. See the module comment for the order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum InvisibleMethod {
    /// The invisible command, XEP-0186.
    Command,
    /// A privacy list, XEP-0016.
    PrivacyList,
}

impl InvisibleMethod {
    /// The mechanism that the server advertises, or `None` if it has none.
    fn from_disco(disco: &disco::State) -> Option<Self> {
        if disco.server_has(NS_INVISIBLE) {
            Some(Self::Command)
        } else if disco.server_has(NS_PRIVACY) {
            Some(Self::PrivacyList)
        } else {
            None
        }
    }
}

/// The presence state of one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The mechanism that hides us now. `None` while we are visible.
    hidden_with: Option<InvisibleMethod>,
    /// When the user stopped interacting, as an xs:dateTime. It stays across sessions.
    pub idle_since: Option<String>,
}

/// The name of the privacy list that makes us invisible.
const INVISIBLE_LIST: &str = "chord-invisible";

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The store of the invisible list. Only an error needs an action.
    StoreList,
    /// The activation of the invisible list, or the invisible command. `reply` is `None`
    /// at connect.
    Hide {
        method: InvisibleMethod,
        reply: Option<Reply<()>>,
    },
    /// The deactivation of the invisible list, or the visible command.
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
    /// Which mechanism hides us. Waits for service discovery.
    InvisibleMethod {
        reply: Reply<Option<InvisibleMethod>>,
    },
    /// Set or clear the idle time, as Unix seconds.
    SetIdle {
        since: Option<i64>,
        reply: Reply<()>,
    },
    /// Turn the version and time answers on or off (version_time.rs). The actor handles it
    /// offline.
    SetShareInfo {
        share: bool,
        reply: Reply<()>,
    },
    /// Turn the read notices and the typing notices on or off. The actor handles it
    /// offline.
    SetNotices {
        read: bool,
        typing: bool,
        reply: Reply<()>,
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

    /// How the server can hide us: `Some` names the mechanism, `None` means that the
    /// server offers none and `Availability::Invisible` does not work. Waits for service
    /// discovery. Fails with `NotConnected` offline.
    pub async fn invisible_method(&self) -> Result<Option<InvisibleMethod>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Presence(Command::InvisibleMethod { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Tell our contacts that we are idle since `since` (Unix seconds), or that we are
    /// not (`None`), with XEP-0319. Chord sends the new presence at once, and puts the
    /// idle time into every later presence of this process. Fails with `NotConnected`
    /// offline: set it again after the next connect.
    pub async fn set_idle(&self, since: Option<i64>) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Presence(Command::SetIdle { since, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Turn the answers to version (XEP-0092) and time (XEP-0202) queries on or off. They
    /// are on by default. Off, Chord answers neither query and leaves both out of its caps
    /// (an online session sends the new presence at once). The choice stays for this
    /// client until the next call: set it before `login` to have it from the first presence.
    pub async fn set_share_info(&self, share: bool) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Presence(Command::SetShareInfo {
            share,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Turn the read notices and the typing notices on or off. Both are on by default.
    /// `read` covers the displayed markers (XEP-0333) and the delivery receipts
    /// (XEP-0184). `typing` covers all chat states (XEP-0085). Off, Chord sends none of
    /// them and still keeps the local read position. The choice stays for this client
    /// until the next call.
    pub async fn set_notices(&self, read: bool, typing: bool) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Presence(Command::SetNotices {
            read,
            typing,
            reply,
        }))?;
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
/// Invisible, it sends nothing now. `on_services_ready` picks the mechanism, hides us, and
/// sends the presence after the answer. The server handles our stanzas in order, so no
/// presence goes out before it.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    if own(ctx).availability == Availability::Invisible {
        return;
    }
    let presence = current(ctx);
    ctx.send(presence);
}

/// Service discovery finished: an invisible session hides us now.
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    if own(ctx).availability == Availability::Invisible && ctx.state.presence.hidden_with.is_none()
    {
        hide(ctx, None);
    }
}

fn own(ctx: &mut Ctx<'_>) -> OwnPresence {
    load(ctx.store, ctx.account_id).unwrap_or_else(|e| {
        ctx.store_error("read our presence", e);
        OwnPresence::default()
    })
}

/// Hide us with the best mechanism that the server advertises. With none, say so, and
/// show us as available.
fn hide(ctx: &mut Ctx<'_>, reply: Option<Reply<()>>) {
    match InvisibleMethod::from_disco(&ctx.state.disco) {
        Some(InvisibleMethod::Command) => {
            log::info!("invisible: XEP-0186 (the server advertises {NS_INVISIBLE})");
            let command = format!("<invisible xmlns='{NS_INVISIBLE}'/>");
            ctx.request(
                set(&command),
                FeaturePending::Presence(Pending::Hide {
                    method: InvisibleMethod::Command,
                    reply,
                }),
            );
        }
        Some(InvisibleMethod::PrivacyList) => {
            log::info!("invisible: XEP-0016 privacy list (the server advertises {NS_PRIVACY})");
            hide_with_list(ctx, reply);
        }
        None => {
            log::info!("invisible: the server offers no mechanism");
            set_availability(ctx, Availability::Available);
            ctx.emit(ClientEvent::Notice(NOT_INVISIBLE.to_owned()));
            broadcast(ctx);
            if let Some(reply) = reply {
                let _ = reply.send(Err(ClientError::Unsupported(
                    "the server has no invisible mode".into(),
                )));
            }
        }
    }
}

const NOT_INVISIBLE: &str = "This server cannot make you invisible. You appear available.";

/// Store the invisible list and make it active. The list stops presence to every contact
/// that has a subscription to it.
fn hide_with_list(ctx: &mut Ctx<'_>, reply: Option<Reply<()>>) {
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
        FeaturePending::Presence(Pending::Hide {
            method: InvisibleMethod::PrivacyList,
            reply,
        }),
    );
}

/// Show us again: the visible command, or no active privacy list.
fn unhide(ctx: &mut Ctx<'_>, method: InvisibleMethod, previous: Availability, reply: Reply<()>) {
    let payload = match method {
        InvisibleMethod::Command => format!("<visible xmlns='{NS_INVISIBLE}'/>"),
        InvisibleMethod::PrivacyList => format!("<query xmlns='{NS_PRIVACY}'><active/></query>"),
    };
    ctx.request(
        set(&payload),
        FeaturePending::Presence(Pending::Show { previous, reply }),
    );
}

/// A privacy list or invisible IQ to our own account. xmpp-parsers 0.23 has no XEP-0016
/// or XEP-0186 types.
fn set(payload: &str) -> Iq {
    Iq::Set {
        from: None,
        to: None,
        id: String::new(),
        payload: payload
            .parse::<Element>()
            .expect("a valid privacy or invisible query"),
    }
}

/// Send our presence to the server, which sends it to the contacts, and to the rooms.
fn broadcast(ctx: &mut Ctx<'_>) {
    // An invisible session sends no presence before the mechanism hides us. The answer to
    // the hide request sends it.
    if own(ctx).availability == Availability::Invisible && ctx.state.presence.hidden_with.is_none()
    {
        return;
    }
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
        Pending::Hide { method, reply } => {
            if ok {
                ctx.state.presence.hidden_with = Some(method);
            } else if !matches!(response, IqResponse::Lost) {
                // The server cannot hide us. Show us as available, and say why.
                set_availability(ctx, Availability::Available);
                ctx.emit(ClientEvent::Notice(NOT_INVISIBLE.to_owned()));
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
                ctx.state.presence.hidden_with = None;
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
    if ctx.state.disco.hide_info {
        // The caps list no version and time: replace the caps element that `initial` made.
        let caps = disco::caps_with(false);
        presence
            .payloads
            .retain(|p| !p.is("c", xmpp_parsers::ns::CAPS));
        presence.payloads.push(caps.into());
    }
    if let Some(since) = &ctx.state.presence.idle_since {
        // `since` is our own xs:dateTime, so it needs no escape.
        let idle = format!("<idle xmlns='{NS_IDLE}' since='{since}'/>");
        presence
            .payloads
            .push(idle.parse().expect("a valid idle element"));
    }
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
                    // The server hides us with the command. A privacy list needs the
                    // unavailable presence from us first.
                    if InvisibleMethod::from_disco(&ctx.state.disco)
                        == Some(InvisibleMethod::PrivacyList)
                    {
                        send_unavailable_to_contacts(ctx);
                    }
                    hide(ctx, Some(reply));
                }
                (true, false) => match ctx.state.presence.hidden_with {
                    Some(method) => unhide(ctx, method, previous, reply),
                    // We never hid: a session that has no mechanism, or that has not yet
                    // run discovery. Nothing to undo.
                    None => {
                        broadcast(ctx);
                        let _ = reply.send(Ok(()));
                    }
                },
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
        Command::InvisibleMethod { reply } => {
            let _ = reply.send(Ok(InvisibleMethod::from_disco(&ctx.state.disco)));
        }
        Command::SetIdle { since, reply } => {
            ctx.state.presence.idle_since = since.map(iso_utc);
            broadcast(ctx);
            let _ = reply.send(Ok(()));
        }
        Command::SetShareInfo { share, reply } => {
            ctx.state.disco.hide_info = !share;
            // The caps changed, so the contacts need a new presence.
            broadcast(ctx);
            let _ = reply.send(Ok(()));
        }
        Command::SetNotices {
            read,
            typing,
            reply,
        } => {
            ctx.state.privacy.no_read_notices = !read;
            ctx.state.privacy.no_typing_notices = !typing;
            let _ = reply.send(Ok(()));
        }
    }
}

/// Unix seconds as an xs:dateTime in UTC (XEP-0082).
pub(crate) fn iso_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
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
        Command::InvisibleMethod { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::SetIdle { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        // The actor keeps the flag offline, so it never gets here.
        Command::SetShareInfo { reply, .. } | Command::SetNotices { reply, .. } => {
            let _ = reply.send(Ok(()));
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

    /// The server advertises `features` and discovery is done.
    fn discover(h: &mut Harness, features: &[&str]) {
        let mut info = crate::features::disco::info(None);
        info.features = features.iter().map(|f| (*f).to_owned()).collect();
        h.state.disco.server = Some(info);
        h.state.disco.complete = true;
    }

    fn invisible_queries(sent: &[Stanza]) -> Vec<Element> {
        sent.iter()
            .filter_map(|s| match s {
                Stanza::Iq(Iq::Set { payload, .. }) if payload.ns() == NS_INVISIBLE => {
                    Some(payload.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn not_supported() -> IqResponse {
        IqResponse::Error(xmpp_parsers::stanza_error::StanzaError::new(
            xmpp_parsers::stanza_error::ErrorType::Cancel,
            xmpp_parsers::stanza_error::DefinedCondition::ServiceUnavailable,
            "en",
            "",
        ))
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
        discover(&mut h, &[NS_PRIVACY]);
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
    fn a_hidden_resource_leaves_the_other_resources_visible() {
        // Two resources of one account. This one hides with the list. The contacts must lose
        // only this resource: one directed unavailable each, to the bare address, and the
        // list is active for this session, never the default list of the account.
        let mut h = Harness::new();
        for (jid, subscription, ask) in [
            ("both@example.org", "both", 0),
            ("from@example.org", "from", 0),
            ("asked@example.org", "none", 1),
            ("to@example.org", "to", 0),
        ] {
            h.store
                .conn()
                .execute(
                    "INSERT INTO contacts (account_id, jid, subscription, ask)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![h.account_id, jid, subscription, ask],
                )
                .unwrap();
        }
        discover(&mut h, &[NS_PRIVACY]);
        let _answer = set_command(&mut h, Availability::Invisible);
        let sent = h.take_sent();
        let gone: Vec<_> = presences(&sent)
            .iter()
            .map(|p| p.to.clone().unwrap())
            .collect();
        assert_eq!(gone.len(), 2);
        assert!(gone.iter().all(|to| to.resource().is_none()), "{gone:?}");
        assert!(gone.iter().all(|to| to.to_bare() != *h.account));
        for query in privacy_queries(&sent) {
            assert!(query.get_child("default", NS_PRIVACY).is_none());
        }
        // Going visible again clears the active list of this session only.
        h.respond(is_hide, IqResponse::Result(None));
        h.take_sent();
        let _answer = set_command(&mut h, Availability::Available);
        for query in privacy_queries(&h.take_sent()) {
            assert!(query.get_child("default", NS_PRIVACY).is_none());
        }
    }

    fn save_invisible(h: &Harness) {
        let invisible = OwnPresence {
            availability: Availability::Invisible,
            status: None,
        };
        save(&h.store, h.account_id, &invisible).unwrap();
    }

    #[test]
    fn an_invisible_session_waits_for_discovery_then_uses_the_privacy_list() {
        let mut h = Harness::new();
        save_invisible(&h);
        h.with_ctx(on_connected);
        assert!(
            h.take_sent().is_empty(),
            "nothing goes out before discovery"
        );

        discover(&mut h, &[NS_PRIVACY]);
        h.with_ctx(on_services_ready);
        let sent = h.take_sent();
        assert!(
            presences(&sent).is_empty(),
            "no presence before the list is active"
        );
        assert_eq!(privacy_queries(&sent).len(), 2);
        assert!(invisible_queries(&sent).is_empty());

        // The list fails: Chord shows us as available and says why.
        h.respond(is_hide, not_supported());
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
    fn the_invisible_command_is_the_first_choice() {
        let mut h = Harness::new();
        // The server offers both: XEP-0186 wins.
        discover(&mut h, &[NS_INVISIBLE, NS_PRIVACY]);
        let mut answer = set_command(&mut h, Availability::Invisible);
        let sent = h.take_sent();
        assert!(privacy_queries(&sent).is_empty());
        assert!(
            presences(&sent).is_empty(),
            "the server hides us, Chord sends no unavailable"
        );
        let queries = invisible_queries(&sent);
        assert_eq!(queries.len(), 1);
        assert!(queries[0].is("invisible", NS_INVISIBLE));
        assert_eq!(answer.try_recv().unwrap(), None);

        h.respond(is_hide, IqResponse::Result(None));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(
            presences(&h.take_sent()).len(),
            1,
            "the initial presence follows"
        );
        assert_eq!(h.state.presence.hidden_with, Some(InvisibleMethod::Command));

        // Going back uses the visible command.
        let mut answer = set_command(&mut h, Availability::Available);
        let sent = h.take_sent();
        assert!(invisible_queries(&sent)[0].is("visible", NS_INVISIBLE));
        assert!(presences(&sent).is_empty());
        h.respond(
            |p| matches!(p, FeaturePending::Presence(Pending::Show { .. })),
            IqResponse::Result(None),
        );
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(h.state.presence.hidden_with, None);
        assert_eq!(presences(&h.take_sent()).len(), 1);
    }

    #[test]
    fn a_server_without_any_mechanism_gets_no_iq_and_we_stay_available() {
        let mut h = Harness::new();
        discover(&mut h, &[]);
        let mut answer = set_command(&mut h, Availability::Invisible);
        let sent = h.take_sent();
        assert!(privacy_queries(&sent).is_empty());
        assert!(invisible_queries(&sent).is_empty());
        assert_eq!(presences(&sent).len(), 1, "an available presence");
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Unsupported(_)))
        ));
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
    fn the_invisible_method_follows_discovery() {
        use crate::features::{FeatureCommand, on_command};
        let mut h = Harness::new();
        let ask = |h: &mut Harness| {
            let (reply, mut answer) = oneshot::channel();
            h.with_ctx(|ctx| {
                on_command(
                    ctx,
                    FeatureCommand::Presence(Command::InvisibleMethod { reply }),
                )
            });
            answer.try_recv().unwrap()
        };
        assert_eq!(ask(&mut h), None, "waits for discovery");
        assert_eq!(h.state.deferred.len(), 1);
        discover(&mut h, &[NS_PRIVACY]);
        assert_eq!(ask(&mut h), Some(Ok(Some(InvisibleMethod::PrivacyList))));
        discover(&mut h, &[NS_PRIVACY, NS_INVISIBLE]);
        assert_eq!(ask(&mut h), Some(Ok(Some(InvisibleMethod::Command))));
        discover(&mut h, &[]);
        assert_eq!(ask(&mut h), Some(Ok(None)));
    }

    #[test]
    fn idle_goes_into_the_presence_until_it_is_cleared() {
        use crate::features::{FeatureCommand, on_command};
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Presence(Command::SetIdle {
                    since: Some(1_700_000_000),
                    reply,
                }),
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        let out = presences(&sent);
        let idle = out[0]
            .payloads
            .iter()
            .find(|p| p.is("idle", NS_IDLE))
            .expect("an idle element");
        assert_eq!(idle.attr("since"), Some("2023-11-14T22:13:20Z"));
        assert!(xmpp_parsers::idle::Idle::try_from(idle.clone()).is_ok());

        // The next session sends it again in the initial presence.
        h.with_ctx(on_connected);
        let sent = h.take_sent();
        assert!(
            presences(&sent)[0]
                .payloads
                .iter()
                .any(|p| p.is("idle", NS_IDLE))
        );

        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Presence(Command::SetIdle { since: None, reply }),
            )
        });
        let sent = h.take_sent();
        assert!(
            !presences(&sent)[0]
                .payloads
                .iter()
                .any(|p| p.is("idle", NS_IDLE))
        );
    }

    #[test]
    fn idle_before_the_hide_answer_sends_no_presence() {
        use crate::features::{FeatureCommand, on_command};
        let mut h = Harness::new();
        save_invisible(&h);
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Presence(Command::SetIdle {
                    since: Some(1_700_000_000),
                    reply,
                }),
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(h.take_sent().is_empty(), "the contacts must not see us");
        // Discovery ends, the server hides us, and the first presence carries the idle time.
        discover(&mut h, &[NS_INVISIBLE]);
        h.with_ctx(on_services_ready);
        h.take_sent();
        h.respond(is_hide, IqResponse::Result(None));
        let sent = h.take_sent();
        assert!(
            presences(&sent)[0]
                .payloads
                .iter()
                .any(|p| p.is("idle", NS_IDLE))
        );
    }

    #[test]
    fn the_date_format_is_xep_0082() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_utc(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn leaving_invisible_deactivates_the_list_first() {
        let mut h = Harness::new();
        let invisible = OwnPresence {
            availability: Availability::Invisible,
            status: None,
        };
        save(&h.store, h.account_id, &invisible).unwrap();
        h.state.presence.hidden_with = Some(InvisibleMethod::PrivacyList);
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
