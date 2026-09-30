//! Profiles: our display name, and the display name of any account.
//!
//! - `set_nickname` publishes our nickname (XEP-0172) to the PEP node
//!   `http://jabber.org/protocol/nick`, in the item `current`. An empty nickname retracts the
//!   item. The server sends it to the contacts that see our presence.
//! - `profile` reads two PEP nodes of an account: the nickname (XEP-0172), then the vCard4
//!   (XEP-0292, node `urn:xmpp:vcard4`). From the vCard4 it takes the formatted name `FN`.
//!   An account with no such node, or a node that we may not read, gives an empty field.
//!   Only a lost connection is an error.
//!
//! The vCard4 write (XEP-0292) and the other vCard4 fields are not here. See
//! `docs/vcard4-plan.md`. The read is on demand: Chord keeps no copy, and it does not ask for
//! `+notify` events of these nodes.

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::ns;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use crate::actor::{ClientError, ClientHandle};

/// The PEP node of the nickname (XEP-0172).
pub const NODE_NICK: &str = "http://jabber.org/protocol/nick";
/// The PEP node of the vCard4 (XEP-0292).
pub const NODE_VCARD4: &str = "urn:xmpp:vcard4";
const NS_NICK: &str = "http://jabber.org/protocol/nick";
const NS_VCARD4: &str = "urn:ietf:params:xml:ns:vcard-4.0";
/// The item id of a PEP node that holds one item.
const ITEM_CURRENT: &str = "current";
/// The longest nickname that we publish, in characters.
pub const MAX_NICKNAME: usize = 128;

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

fn nc(name: &'static str) -> NcName {
    NcName::try_from(name).expect("a valid attribute name")
}

/// What an account tells about itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct Profile {
    /// The nickname of XEP-0172.
    pub nickname: Option<String>,
    /// The formatted name `FN` of the vCard4 (XEP-0292).
    pub full_name: Option<String>,
}

impl Profile {
    /// The name to show: the nickname, otherwise the formatted name.
    pub fn display_name(&self) -> Option<&str> {
        self.nickname.as_deref().or(self.full_name.as_deref())
    }
}

pub(crate) enum Command {
    SetNickname {
        nickname: Option<String>,
        reply: Reply<()>,
    },
    Get {
        jid: BareJid,
        reply: Reply<Profile>,
    },
}

#[derive(Debug)]
pub(crate) enum Pending {
    Publish {
        reply: Reply<()>,
    },
    Nick {
        jid: BareJid,
        reply: Reply<Profile>,
    },
    VCard4 {
        nickname: Option<String>,
        reply: Reply<Profile>,
    },
}

impl ClientHandle {
    /// Publish our nickname (XEP-0172). `None` or an empty text removes it. Fails with
    /// `Invalid` for a text of more than 128 characters.
    pub async fn set_nickname(&self, nickname: Option<String>) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Profile(Command::SetNickname {
            nickname,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Read the nickname and the vCard4 name of an account (XEP-0172, XEP-0292). The
    /// account can be our own.
    pub async fn profile(&self, jid: BareJid) -> Result<Profile, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Profile(Command::Get { jid, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::SetNickname { nickname, reply } => {
            let nickname = nickname
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty());
            if nickname
                .as_ref()
                .is_some_and(|n| n.chars().count() > MAX_NICKNAME)
            {
                let _ = reply.send(Err(ClientError::Invalid("the nickname is too long".into())));
                return;
            }
            let action = match nickname {
                Some(n) => Element::builder("publish", ns::PUBSUB)
                    .attr(nc("node"), NODE_NICK)
                    .append(
                        Element::builder("item", ns::PUBSUB)
                            .attr(nc("id"), ITEM_CURRENT)
                            .append(Element::builder("nick", NS_NICK).append(n)),
                    ),
                None => Element::builder("retract", ns::PUBSUB)
                    .attr(nc("node"), NODE_NICK)
                    .attr(nc("notify"), "true")
                    .append(Element::builder("item", ns::PUBSUB).attr(nc("id"), ITEM_CURRENT)),
            };
            let payload = Element::builder("pubsub", ns::PUBSUB)
                .append(action)
                .build();
            // No `to`: the request goes to our own account (XEP-0163).
            let iq = Iq::Set {
                from: None,
                to: None,
                id: String::new(),
                payload,
            };
            ctx.request(iq, FeaturePending::Profile(Pending::Publish { reply }));
        }
        Command::Get { jid, reply } => {
            ctx.request(
                items_iq(&jid, NODE_NICK),
                FeaturePending::Profile(Pending::Nick { jid, reply }),
            );
        }
    }
}

/// The IQ that reads the newest item of a PEP node (XEP-0163, XEP-0060 6.5).
fn items_iq(jid: &BareJid, node: &str) -> Iq {
    let items = Element::builder("items", ns::PUBSUB)
        .attr(nc("node"), node)
        .attr(nc("max_items"), "1");
    Iq::Get {
        from: None,
        to: Some(Jid::from(jid.clone())),
        id: String::new(),
        payload: Element::builder("pubsub", ns::PUBSUB).append(items).build(),
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Publish { reply } => {
            let _ = reply.send(match response {
                IqResponse::Result(_) => Ok(()),
                // Retracting an item that is not there is fine.
                IqResponse::Error(e)
                    if e.defined_condition
                        == xmpp_parsers::stanza_error::DefinedCondition::ItemNotFound =>
                {
                    Ok(())
                }
                IqResponse::Error(e) => {
                    Err(ClientError::Server(format!("{:?}", e.defined_condition)))
                }
                IqResponse::Lost => Err(ClientError::NotConnected),
            });
        }
        Pending::Nick { jid, reply } => {
            if matches!(response, IqResponse::Lost) {
                let _ = reply.send(Err(ClientError::NotConnected));
                return;
            }
            let nickname = match response {
                IqResponse::Result(Some(p)) => nickname_of(&p),
                _ => None,
            };
            ctx.request(
                items_iq(&jid, NODE_VCARD4),
                FeaturePending::Profile(Pending::VCard4 { nickname, reply }),
            );
        }
        Pending::VCard4 { nickname, reply } => {
            let _ = reply.send(match response {
                IqResponse::Lost => Err(ClientError::NotConnected),
                IqResponse::Result(Some(p)) => Ok(Profile {
                    nickname,
                    full_name: full_name_of(&p),
                }),
                _ => Ok(Profile {
                    nickname,
                    full_name: None,
                }),
            });
        }
    }
}

pub(crate) fn offline(command: Command) {
    match command {
        Command::SetNickname { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::Get { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// The payload of the first item of a pubsub items result.
fn first_item(payload: &Element) -> Option<&Element> {
    payload
        .get_child("items", ns::PUBSUB)?
        .get_child("item", ns::PUBSUB)?
        .children()
        .next()
}

fn clean(text: String) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The nickname in a XEP-0172 items result.
fn nickname_of(payload: &Element) -> Option<String> {
    let nick = first_item(payload)?;
    nick.is("nick", NS_NICK)
        .then(|| nick.text())
        .and_then(clean)
}

/// The `FN` in a XEP-0292 items result: `<vcard><fn><text>NAME</text></fn></vcard>`.
fn full_name_of(payload: &Element) -> Option<String> {
    let vcard = first_item(payload)?;
    if !vcard.is("vcard", NS_VCARD4) {
        return None;
    }
    clean(
        vcard
            .get_child("fn", NS_VCARD4)?
            .get_child("text", NS_VCARD4)?
            .text(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn items(node: &str, inner: &str) -> Element {
        el(&format!(
            "<pubsub xmlns='{}'><items node='{node}'><item id='current'>{inner}</item></items></pubsub>",
            ns::PUBSUB
        ))
    }

    fn is_publish(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Profile(Pending::Publish { .. }))
    }

    fn is_nick(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Profile(Pending::Nick { .. }))
    }

    fn is_vcard(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Profile(Pending::VCard4 { .. }))
    }

    fn set(h: &mut Harness, nickname: Option<&str>) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        let nickname = nickname.map(str::to_owned);
        h.with_ctx(|ctx| on_command(ctx, Command::SetNickname { nickname, reply }));
        answer
    }

    #[test]
    fn set_nickname_publishes_the_item_to_our_own_pep() {
        let mut h = Harness::new();
        let mut answer = set(&mut h, Some("  Alice A  "));
        let sent = h.sent_iqs();
        let [Iq::Set { to, payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert!(to.is_none());
        let publish = payload.get_child("publish", ns::PUBSUB).unwrap();
        assert_eq!(publish.attr("node"), Some(NODE_NICK));
        let item = publish.get_child("item", ns::PUBSUB).unwrap();
        assert_eq!(item.attr("id"), Some("current"));
        assert_eq!(item.get_child("nick", NS_NICK).unwrap().text(), "Alice A");
        assert_eq!(answer.try_recv().unwrap(), None);
        h.answer(is_publish, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn an_empty_nickname_retracts_the_item_and_a_missing_item_is_fine() {
        let mut h = Harness::new();
        let mut answer = set(&mut h, Some("   "));
        let sent = h.sent_iqs();
        let [Iq::Set { payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        let retract = payload.get_child("retract", ns::PUBSUB).unwrap();
        assert_eq!(retract.attr("node"), Some(NODE_NICK));
        assert_eq!(retract.attr("notify"), Some("true"));
        h.respond(
            is_publish,
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ItemNotFound,
                "en",
                "",
            )),
        );
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn set_nickname_errors() {
        let mut h = Harness::new();
        let mut answer = set(&mut h, Some(&"x".repeat(MAX_NICKNAME + 1)));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert!(h.sent_iqs().is_empty());
        let mut answer = set(&mut h, Some("Al"));
        h.respond(
            is_publish,
            IqResponse::Error(StanzaError::new(
                ErrorType::Auth,
                DefinedCondition::Forbidden,
                "en",
                "",
            )),
        );
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server("Forbidden".into())))
        );
        let mut answer = set(&mut h, Some("Al"));
        h.respond(is_publish, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    fn get(h: &mut Harness, jid: &str) -> oneshot::Receiver<Result<Profile, ClientError>> {
        let (reply, answer) = oneshot::channel();
        let jid = BareJid::new(jid).unwrap();
        h.with_ctx(|ctx| on_command(ctx, Command::Get { jid, reply }));
        answer
    }

    #[test]
    fn profile_reads_the_nickname_and_the_vcard4_name() {
        let mut h = Harness::new();
        let mut answer = get(&mut h, "bob@chord.localhost");
        let sent = h.sent_iqs();
        let [Iq::Get { to, payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert_eq!(to.as_ref().unwrap().as_str(), "bob@chord.localhost");
        let q = payload.get_child("items", ns::PUBSUB).unwrap();
        assert_eq!(q.attr("node"), Some(NODE_NICK));
        h.answer(
            is_nick,
            Some(items(
                NODE_NICK,
                &format!("<nick xmlns='{NS_NICK}'>Bobby</nick>"),
            )),
        );
        let sent = h.sent_iqs();
        let [Iq::Get { payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert_eq!(
            payload.get_child("items", ns::PUBSUB).unwrap().attr("node"),
            Some(NODE_VCARD4)
        );
        h.answer(
            is_vcard,
            Some(items(
                NODE_VCARD4,
                &format!(
                    "<vcard xmlns='{NS_VCARD4}'><fn><text>Robert Bob</text></fn>
                     <bday><date>1990-01-02</date></bday></vcard>"
                ),
            )),
        );
        let profile = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(profile.nickname.as_deref(), Some("Bobby"));
        assert_eq!(profile.full_name.as_deref(), Some("Robert Bob"));
        assert_eq!(profile.display_name(), Some("Bobby"));
    }

    #[test]
    fn profile_falls_back_to_the_vcard4_name_and_survives_errors() {
        let mut h = Harness::new();
        let mut answer = get(&mut h, "bob@chord.localhost");
        h.sent_iqs();
        h.respond(
            is_nick,
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ItemNotFound,
                "en",
                "",
            )),
        );
        h.answer(
            is_vcard,
            Some(items(
                NODE_VCARD4,
                &format!("<vcard xmlns='{NS_VCARD4}'><fn><text> Robert </text></fn></vcard>"),
            )),
        );
        let profile = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(profile.nickname, None);
        assert_eq!(profile.display_name(), Some("Robert"));
        // No nodes at all: an empty profile, no error.
        let mut answer = get(&mut h, "eve@chord.localhost");
        h.answer(is_nick, Some(items(NODE_NICK, "<other xmlns='x'/>")));
        h.respond(
            is_vcard,
            IqResponse::Error(StanzaError::new(
                ErrorType::Auth,
                DefinedCondition::Forbidden,
                "en",
                "",
            )),
        );
        assert_eq!(
            answer.try_recv().unwrap().unwrap().unwrap(),
            Profile::default()
        );
        // A lost connection is an error.
        let mut answer = get(&mut h, "eve@chord.localhost");
        h.respond(is_nick, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }
}
