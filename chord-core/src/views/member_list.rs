//! `MemberList`: the occupants of a room, with presence. For a 1:1 chat: both people.

use jid::BareJid;
use rusqlite::{OptionalExtension, params};

use super::timeline::{avatar_hash, contact_name, local_part};
use super::{QueryCtx, ViewItem};

#[derive(Clone, Debug, PartialEq)]
pub struct MemberItem {
    /// Stable id: the nick in a room, the bare JID in a 1:1 chat.
    pub id: String,
    pub name: String,
    /// Real bare JID, if known.
    pub jid: Option<String>,
    /// MUC role: moderator, participant, or visitor. `participant` in a 1:1 chat.
    pub role: String,
    /// MUC affiliation: owner, admin, member, or none.
    pub affiliation: String,
    /// Presence show: away, chat, dnd, xa. `None` means available or offline.
    pub show: Option<String>,
    pub online: bool,
    pub avatar: Option<String>,
}

impl ViewItem for MemberItem {
    type Key = String;
    fn key(&self) -> String {
        self.id.clone()
    }
}

pub(crate) fn query(q: &QueryCtx<'_>, room: &BareJid) -> rusqlite::Result<Vec<MemberItem>> {
    let is_room: bool = q
        .store
        .conn()
        .prepare_cached("SELECT 1 FROM rooms WHERE account_id = ?1 AND jid = ?2")?
        .query_row(params![q.account_id, room.as_str()], |_| Ok(()))
        .optional()?
        .is_some();
    if is_room {
        occupants(q, room)
    } else {
        direct(q, room)
    }
}

/// nick, real JID, role, affiliation, show.
type OccupantRow = (String, Option<String>, String, String, Option<String>);

/// Room occupants: moderators first, then participants, then visitors, by name.
fn occupants(q: &QueryCtx<'_>, room: &BareJid) -> rusqlite::Result<Vec<MemberItem>> {
    let mut stmt = q.store.conn().prepare_cached(
        "SELECT nick, real_jid, role, affiliation, show FROM occupants
         WHERE account_id = ?1 AND room = ?2
         ORDER BY CASE role WHEN 'moderator' THEN 0 WHEN 'participant' THEN 1 ELSE 2 END,
                  nick COLLATE NOCASE",
    )?;
    let rows: Vec<OccupantRow> = stmt
        .query_map(params![q.account_id, room.as_str()], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (nick, jid, role, affiliation, show) in rows {
        let avatar = avatar_hash(q, &format!("{}/{nick}", room.as_str()))?;
        out.push(MemberItem {
            id: nick.clone(),
            name: nick,
            jid,
            role,
            affiliation,
            show,
            online: true,
            avatar,
        });
    }
    Ok(out)
}

/// A 1:1 chat: the account, then the peer.
fn direct(q: &QueryCtx<'_>, peer: &BareJid) -> rusqlite::Result<Vec<MemberItem>> {
    let me = q.account.as_str();
    let presence: Option<Option<String>> = q
        .store
        .conn()
        .prepare_cached(
            "SELECT show FROM presences WHERE account_id = ?1 AND bare = ?2
             ORDER BY priority DESC LIMIT 1",
        )?
        .query_row(params![q.account_id, peer.as_str()], |row| row.get(0))
        .optional()?;
    Ok(vec![
        MemberItem {
            id: me.to_owned(),
            name: local_part(me),
            jid: Some(me.to_owned()),
            role: "participant".into(),
            affiliation: "none".into(),
            show: None,
            online: true,
            avatar: avatar_hash(q, me)?,
        },
        MemberItem {
            id: peer.to_string(),
            name: contact_name(q, peer.as_str())?,
            jid: Some(peer.to_string()),
            role: "participant".into(),
            affiliation: "none".into(),
            online: presence.is_some(),
            show: presence.flatten(),
            avatar: avatar_hash(q, peer.as_str())?,
        },
    ])
}
