//! Presence (RFC 6121): our own presence. Contact presence is in `roster`, room presence
//! in `muc`.

use xmpp_parsers::presence::Presence;

use super::{Ctx, avatars, disco};

/// Send initial presence (RFC 6121, 4.2) with our entity capabilities (XEP-0115). The
/// server routes chat messages to a resource only after it is available. PEP sends
/// notifications (`+notify`) only to clients whose caps ask for them.
///
/// If the store has our avatar, the presence also carries its hash (XEP-0153), so that
/// vCard clients see it.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    let own = match avatars::load(ctx.store, ctx.account_id, ctx.account) {
        Ok(avatar) => avatar,
        Err(e) => {
            ctx.store_error("read our avatar", e);
            None
        }
    };
    match own {
        Some(avatar) => ctx.send(avatars::vcard_presence(Some(&avatar.hash))),
        None => ctx.send(initial()),
    }
}

/// Available presence with our caps.
pub fn initial() -> Presence {
    Presence::available().with_payloads(vec![disco::caps().into()])
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
}
