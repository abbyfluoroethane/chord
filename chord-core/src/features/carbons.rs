//! Message carbons (XEP-0280): copies of the messages of the other clients of the account.

use xmpp_parsers::carbons::{Enable, Received, Sent};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::Message;

use super::{Ctx, IqResponse, Pending};

/// Turn carbons on for this session.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    ctx.request(Iq::from_set("", Enable), Pending::Carbons);
}

pub(crate) fn on_enabled(_ctx: &mut Ctx<'_>, response: IqResponse) {
    if let IqResponse::Error(e) = response {
        log::warn!("cannot enable message carbons: {e:?}");
    }
}

/// If `message` is a carbon, return the message inside it. Return any other message as
/// it is. Return `None` for a carbon that is not from our own account: anyone can send a
/// forged carbon (XEP-0280, section 11).
pub(crate) fn unwrap(ctx: &mut Ctx<'_>, message: Message) -> Option<Message> {
    let from_own = message
        .from
        .as_ref()
        .is_some_and(|f| f.to_bare() == *ctx.account)
        || message.from.is_none();
    for payload in &message.payloads {
        let forwarded = if let Ok(received) = Received::try_from(payload.clone()) {
            received.forwarded
        } else if let Ok(sent) = Sent::try_from(payload.clone()) {
            sent.forwarded
        } else {
            continue;
        };
        if !from_own {
            log::warn!(
                "dropped a carbon from {:?}: not from our account",
                message.from
            );
            return None;
        }
        let mut inner = forwarded.message;
        if let Some(delay) = forwarded.delay {
            inner = inner.with_payload(delay);
        }
        return Some(inner);
    }
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use jid::Jid;
    use xmpp_parsers::forwarding::Forwarded;

    fn chat(from: &str, to: &str) -> Message {
        let mut m = Message::chat(Jid::new(to).unwrap()).with_body("".into(), "hi".into());
        m.from = Some(Jid::new(from).unwrap());
        m
    }

    #[test]
    fn carbon_from_own_account_is_unwrapped() {
        let mut h = Harness::new();
        let inner = chat("bob@chord.localhost/phone", "alice@chord.localhost/phone");
        let mut carbon = Message::normal(Jid::new("alice@chord.localhost/chord").unwrap())
            .with_payload(Received {
                forwarded: Forwarded {
                    delay: None,
                    message: inner,
                },
            });
        carbon.from = Some(Jid::new("alice@chord.localhost").unwrap());
        let out = h.with_ctx(|ctx| unwrap(ctx, carbon)).unwrap();
        assert_eq!(
            out.from,
            Some(Jid::new("bob@chord.localhost/phone").unwrap())
        );
    }

    #[test]
    fn carbon_from_someone_else_is_dropped() {
        let mut h = Harness::new();
        let inner = chat("alice@chord.localhost/phone", "bob@chord.localhost");
        let mut carbon = Message::normal(Jid::new("alice@chord.localhost/chord").unwrap())
            .with_payload(Sent {
                forwarded: Forwarded {
                    delay: None,
                    message: inner,
                },
            });
        carbon.from = Some(Jid::new("mallory@evil.example").unwrap());
        assert!(h.with_ctx(|ctx| unwrap(ctx, carbon)).is_none());
    }
}
