//! The status codes of a room (XEP-0045, section 15.6), read from the raw XML.
//!
//! `xmpp-parsers` knows some of the codes only. A presence with a code that it does not
//! know (174) fails to parse as a whole, and the codes that it does know are lost with it.

use jid::BareJid;
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::minidom::Element;
use xmpp_parsers::muc::user::MucUser;

use super::{Ctx, IqResponse, NS_DISCO_INFO, NS_MUC_USER, Pending, db, is_joined_or_joining};
use crate::actor::ClientEvent;

/// The numbers of the `status` children of the `muc#user` payloads.
pub(super) fn codes(payloads: &[Element]) -> Vec<u16> {
    payloads
        .iter()
        .filter(|x| x.is("x", NS_MUC_USER))
        .flat_map(Element::children)
        .filter(|c| c.is("status", NS_MUC_USER))
        .filter_map(|c| c.attr("code")?.parse().ok())
        .collect()
}

/// The `muc#user` payload, with the status codes that `xmpp-parsers` does not know left out.
pub(super) fn muc_user(payloads: &[Element]) -> Option<MucUser> {
    let x = payloads.iter().find(|x| x.is("x", NS_MUC_USER))?;
    let mut clean = Element::builder("x", NS_MUC_USER);
    for child in x.children() {
        let known = !child.is("status", NS_MUC_USER)
            || xmpp_parsers::muc::user::Status::try_from(child.clone()).is_ok();
        if known {
            clean = clean.append(child.clone());
        }
    }
    MucUser::try_from(clean.build()).ok()
}

/// One plain sentence for the reason that the room removed us. `codes` are the status
/// codes of the presence. `reason` is the text that the moderator gave, if any.
pub(super) fn removal_text(room: &BareJid, codes: &[u16], reason: Option<&str>) -> String {
    let what = if codes.contains(&301) {
        format!("You were banned from {room}")
    } else if codes.contains(&307) {
        format!("You were kicked from {room}")
    } else if codes.contains(&321) {
        format!("You were removed from {room} because your affiliation changed")
    } else if codes.contains(&322) {
        format!("You were removed from {room} because it is now for members only")
    } else if codes.contains(&332) {
        format!("You were removed from {room} because the chat service is shutting down")
    } else if codes.contains(&333) {
        format!("You were removed from {room} because of a technical error in the chat service")
    } else {
        format!("You were removed from {room}")
    };
    match reason {
        Some(reason) => format!("{what}: {reason}"),
        None => what,
    }
}

/// Whether the service will let us in again soon: it shuts down, or it had an error.
pub(super) fn is_temporary(codes: &[u16]) -> bool {
    codes.contains(&332) || codes.contains(&333)
}

/// The anonymity that a status code states.
fn anonymity_of(code: u16) -> Option<&'static str> {
    match code {
        100 | 172 => Some("non-anonymous"),
        173 => Some("semi-anonymous"),
        174 => Some("anonymous"),
        _ => None,
    }
}

/// Status codes about the room itself. They come in a message from the room after a
/// configuration change, and in our own presence when we join.
pub(super) fn on_room_codes(ctx: &mut Ctx<'_>, room: &BareJid, codes: &[u16], announced: bool) {
    for &code in codes {
        if let Some(anonymity) = anonymity_of(code)
            && set_anonymity(ctx, room, anonymity)
            && announced
        {
            ctx.emit(ClientEvent::Notice(match code {
                172 => format!("{room} is now non-anonymous: everyone can see your address"),
                173 => {
                    format!("{room} is now semi-anonymous: only moderators can see your address")
                }
                _ => format!("{room} is now anonymous: nobody can see your address"),
            }));
        }
        match code {
            // A configuration change that does not touch privacy: read the room again.
            104 if announced => refresh(ctx, room),
            170 if announced => {
                ctx.emit(ClientEvent::Notice(format!(
                    "{room} now keeps a public log"
                )));
            }
            171 if announced => {
                ctx.emit(ClientEvent::Notice(format!(
                    "{room} no longer keeps a public log"
                )));
            }
            _ => {}
        }
    }
}

/// Store the anonymity of a room. Returns true when it changed.
fn set_anonymity(ctx: &Ctx<'_>, room: &BareJid, anonymity: &str) -> bool {
    let old: Option<String> = db(
        ctx,
        "read the anonymity of a room",
        ctx.store
            .conn()
            .query_row(
                "SELECT anonymity FROM rooms WHERE account_id = ?1 AND jid = ?2",
                params![ctx.account_id, room.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional(),
    )
    .flatten()
    .flatten();
    if old.as_deref() == Some(anonymity) {
        return false;
    }
    db(
        ctx,
        "store the anonymity of a room",
        ctx.store.conn().execute(
            "UPDATE rooms SET anonymity = ?3 WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, room.as_str(), anonymity],
        ),
    );
    true
}

/// Read the disco#info of a room again (XEP-0045, 10.2.1).
fn refresh(ctx: &mut Ctx<'_>, room: &BareJid) {
    if !is_joined_or_joining(ctx, room) || !ctx.state.muc.refreshing.insert(room.clone()) {
        return;
    }
    let iq = xmpp_parsers::iq::Iq::Get {
        from: None,
        to: Some(jid::Jid::from(room.clone())),
        id: String::new(),
        payload: Element::builder("query", NS_DISCO_INFO).build(),
    };
    ctx.request(
        iq,
        crate::features::Pending::Muc(Pending::Refresh(room.clone())),
    );
}

/// The answer to `refresh`: keep the anonymity that the room states now.
pub(super) fn on_refreshed(ctx: &mut Ctx<'_>, room: BareJid, response: IqResponse) {
    ctx.state.muc.refreshing.remove(&room);
    match response {
        IqResponse::Result(Some(query)) => match super::room_card(&room, query) {
            Ok(card) => {
                if let Some(anonymity) = card.anonymity {
                    set_anonymity(ctx, &room, &anonymity);
                }
                super::mark_room(ctx, &room);
            }
            Err(e) => log::debug!("the new disco#info of {room} is no room: {e}"),
        },
        IqResponse::Result(None) | IqResponse::Lost => {}
        IqResponse::Error(e) => {
            log::debug!(
                "cannot read the disco#info of {room}: {}",
                super::error_text(&e)
            );
        }
    }
}
