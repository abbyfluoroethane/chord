//! Presence (RFC 6121): our own presence. Contact presence is in `roster`, room presence
//! in `muc`.
//!
//! Our show value (away, dnd, xa) and status text live in the `own_presence` table, so
//! every presence and room join carries them, also after a restart.

use futures_channel::oneshot;
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::presence::{Presence, Show};

use super::{Ctx, FeatureCommand, avatars, disco};
use crate::actor::{ClientError, ClientHandle};
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
}

impl Availability {
    fn show(self) -> Option<Show> {
        match self {
            Self::Available => None,
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
        }
    }

    fn parse(text: Option<&str>) -> Self {
        match text {
            Some("away") => Self::Away,
            Some("dnd") => Self::Dnd,
            Some("xa") => Self::ExtendedAway,
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

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

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
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    let presence = current(ctx);
    ctx.send(presence);
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
    let own = load(ctx.store, ctx.account_id).unwrap_or_else(|e| {
        ctx.store_error("read our presence", e);
        OwnPresence::default()
    });
    presence.show = own.availability.show();
    if let Some(status) = own.status.filter(|s| !s.is_empty()) {
        presence.set_status("", status);
    }
    presence
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Set { presence, reply } => {
            if let Err(e) = save(ctx.store, ctx.account_id, &presence) {
                let _ = reply.send(Err(ClientError::Invalid(format!("store: {e}"))));
                return;
            }
            let broadcast = current(ctx);
            ctx.send(broadcast.clone());
            super::muc::send_presence_to_rooms(ctx, &broadcast);
            let _ = reply.send(Ok(()));
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
}
