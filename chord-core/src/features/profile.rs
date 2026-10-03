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
//! - `set_profile` writes the vCard4 fields that the user edits: `FN`, `note` (about me),
//!   `url` and `pronouns` (RFC 9554). It reads our own item first, changes only these four
//!   properties and publishes the whole item again, so other properties stay.
//! - `own_devices` lists the resources of our own account that are online.
//!
//! The read is on demand: Chord keeps no copy, and it does not ask for
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
/// The longest about text of the vCard4, in characters.
pub const MAX_ABOUT: usize = 2000;
/// The longest website of the vCard4, in characters.
pub const MAX_WEBSITE: usize = 512;
/// The longest pronouns text of the vCard4, in characters.
pub const MAX_PRONOUNS: usize = 64;

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
    /// The `note` of the vCard4: the text "about me".
    pub about: Option<String>,
    /// The `url` of the vCard4.
    pub website: Option<String>,
    /// The `pronouns` of the vCard4 (RFC 9554).
    pub pronouns: Option<String>,
}

/// The vCard4 fields that the user edits. A field that is `None` or empty is removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize),
    serde(rename_all = "camelCase", default)
)]
pub struct ProfileEdit {
    pub full_name: Option<String>,
    pub about: Option<String>,
    pub website: Option<String>,
    pub pronouns: Option<String>,
}

/// One resource of our own account that is online.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct Device {
    pub resource: String,
    /// The `show` value of its presence: away, chat, dnd or xa. `None` means available.
    pub show: Option<String>,
    pub status: Option<String>,
    pub priority: i8,
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
    SetVCard {
        edit: ProfileEdit,
        reply: Reply<()>,
    },
    Devices {
        reply: Reply<Vec<Device>>,
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
    /// The read of our own vCard4 before a write.
    VCardRead {
        edit: ProfileEdit,
        reply: Reply<()>,
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

    /// Write the vCard4 fields of our own account (XEP-0292). Other properties of the item
    /// stay. Fails with `Invalid` for a text that is too long or a website that is not an
    /// http or https address.
    pub async fn set_profile(&self, edit: ProfileEdit) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Profile(Command::SetVCard { edit, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The resources of our own account that are online, from their presence. It includes
    /// this session. The list is empty offline.
    pub async fn own_devices(&self) -> Result<Vec<Device>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Profile(Command::Devices { reply }))?;
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
        Command::SetVCard { edit, reply } => match clean_edit(edit) {
            Ok(edit) => {
                let iq = items_iq(ctx.account, NODE_VCARD4);
                ctx.request(
                    iq,
                    FeaturePending::Profile(Pending::VCardRead { edit, reply }),
                );
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        },
        Command::Devices { reply } => {
            let _ = reply.send(
                devices(ctx.store.conn(), ctx.account_id, ctx.account)
                    .map_err(|e| ClientError::Invalid(format!("store: {e}"))),
            );
        }
    }
}

/// Trim the fields, turn an empty field into `None` and check the limits.
fn clean_edit(edit: ProfileEdit) -> Result<ProfileEdit, ClientError> {
    let trim = |t: Option<String>| t.and_then(clean);
    let edit = ProfileEdit {
        full_name: trim(edit.full_name),
        about: trim(edit.about),
        website: trim(edit.website),
        pronouns: trim(edit.pronouns),
    };
    let too_long =
        |t: &Option<String>, max: usize| t.as_ref().is_some_and(|t| t.chars().count() > max);
    if too_long(&edit.full_name, MAX_NICKNAME)
        || too_long(&edit.about, MAX_ABOUT)
        || too_long(&edit.website, MAX_WEBSITE)
        || too_long(&edit.pronouns, MAX_PRONOUNS)
    {
        return Err(ClientError::Invalid("a profile field is too long".into()));
    }
    if let Some(site) = &edit.website {
        let lower = site.to_ascii_lowercase();
        let scheme = lower.starts_with("https://") || lower.starts_with("http://");
        if !scheme || site.contains(char::is_whitespace) {
            return Err(ClientError::Invalid(
                "the website must be an http or https address".into(),
            ));
        }
    }
    Ok(edit)
}

/// A vCard4 property with one text child.
fn prop(name: &str, inner: &str, text: &str) -> Element {
    Element::builder(name, NS_VCARD4)
        .append(Element::builder(inner, NS_VCARD4).append(text.to_owned()))
        .build()
}

/// The vCard4 to publish: the old item with the four edited properties replaced. `None`
/// means that nothing is left, so the item goes.
fn merged_vcard(old: Option<&Element>, edit: &ProfileEdit, account: &BareJid) -> Option<Element> {
    let old = old.filter(|v| v.is("vcard", NS_VCARD4));
    let old_name = old.and_then(|v| text_of(v, "fn", "text"));
    // The properties that stay, in their order.
    let kept: Vec<Element> = old
        .map(|v| {
            v.children()
                .filter(|c| {
                    !(c.ns() == NS_VCARD4 && matches!(c.name(), "fn" | "note" | "url" | "pronouns"))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let edited = edit.about.is_some() || edit.website.is_some() || edit.pronouns.is_some();
    // A vCard4 needs `FN`. Keep the old one, or take the local part of our address.
    let name = edit.full_name.clone().or_else(|| {
        (edited || !kept.is_empty()).then(|| {
            old_name.unwrap_or_else(|| account.node().map(|n| n.to_string()).unwrap_or_default())
        })
    });
    let mut out = Element::builder("vcard", NS_VCARD4).build();
    if let Some(n) = &name {
        out.append_child(prop("fn", "text", n));
    }
    for e in kept {
        out.append_child(e);
    }
    if let Some(t) = &edit.about {
        out.append_child(prop("note", "text", t));
    }
    if let Some(t) = &edit.website {
        out.append_child(prop("url", "uri", t));
    }
    if let Some(t) = &edit.pronouns {
        out.append_child(prop("pronouns", "text", t));
    }
    out.children().next().is_some().then_some(out)
}

/// Our own resources that have a presence in the store, sorted by resource.
fn devices(
    conn: &rusqlite::Connection,
    account_id: i64,
    account: &BareJid,
) -> rusqlite::Result<Vec<Device>> {
    let mut stmt = conn.prepare(
        "SELECT jid, show, status, priority FROM presences
         WHERE account_id = ?1 AND bare = ?2 ORDER BY jid",
    )?;
    let rows = stmt.query_map(rusqlite::params![account_id, account.as_str()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (jid, show, status, priority) = row?;
        // A row with no resource is not a device.
        let Some((_, resource)) = jid.split_once('/') else {
            continue;
        };
        out.push(Device {
            resource: resource.to_owned(),
            show,
            status,
            priority: i8::try_from(priority).unwrap_or(0),
        });
    }
    Ok(out)
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
                IqResponse::Result(Some(p)) => Ok(profile_of(nickname, &p)),
                _ => Ok(Profile {
                    nickname,
                    ..Profile::default()
                }),
            });
        }
        Pending::VCardRead { edit, reply } => {
            // A missing node or item is an empty vCard. Any other error stops the write:
            // we must not replace an item that we could not read.
            let old = match &response {
                IqResponse::Lost => {
                    let _ = reply.send(Err(ClientError::NotConnected));
                    return;
                }
                IqResponse::Result(p) => p.as_ref().and_then(first_item).cloned(),
                IqResponse::Error(e)
                    if matches!(
                        e.defined_condition,
                        xmpp_parsers::stanza_error::DefinedCondition::ItemNotFound
                    ) =>
                {
                    None
                }
                IqResponse::Error(e) => {
                    let condition = format!("{:?}", e.defined_condition);
                    let _ = reply.send(Err(ClientError::Server(condition)));
                    return;
                }
            };
            let action = match merged_vcard(old.as_ref(), &edit, ctx.account) {
                Some(vcard) => Element::builder("publish", ns::PUBSUB)
                    .attr(nc("node"), NODE_VCARD4)
                    .append(
                        Element::builder("item", ns::PUBSUB)
                            .attr(nc("id"), ITEM_CURRENT)
                            .append(vcard),
                    ),
                None => Element::builder("retract", ns::PUBSUB)
                    .attr(nc("node"), NODE_VCARD4)
                    .attr(nc("notify"), "true")
                    .append(Element::builder("item", ns::PUBSUB).attr(nc("id"), ITEM_CURRENT)),
            };
            let iq = Iq::Set {
                from: None,
                to: None,
                id: String::new(),
                payload: Element::builder("pubsub", ns::PUBSUB)
                    .append(action)
                    .build(),
            };
            ctx.request(iq, FeaturePending::Profile(Pending::Publish { reply }));
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
        Command::SetVCard { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::Devices { reply } => {
            let _ = reply.send(Ok(Vec::new()));
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

/// The text of a vCard4 property: `<name><INNER>TEXT</INNER></name>`.
fn text_of(vcard: &Element, name: &str, inner: &str) -> Option<String> {
    clean(
        vcard
            .get_child(name, NS_VCARD4)?
            .get_child(inner, NS_VCARD4)?
            .text(),
    )
}

/// The fields that we show, from a XEP-0292 items result: `<vcard><fn><text>NAME</text></fn>`.
fn profile_of(nickname: Option<String>, payload: &Element) -> Profile {
    let mut profile = Profile {
        nickname,
        ..Profile::default()
    };
    if let Some(vcard) = first_item(payload).filter(|v| v.is("vcard", NS_VCARD4)) {
        profile.full_name = text_of(vcard, "fn", "text");
        profile.about = text_of(vcard, "note", "text");
        profile.website = text_of(vcard, "url", "uri");
        profile.pronouns = text_of(vcard, "pronouns", "text");
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

    const ACCOUNT_BARE: &str = "alice@chord.localhost";

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

    fn edit(name: &str, about: &str, site: &str, pronouns: &str) -> ProfileEdit {
        let o = |t: &str| Some(t.to_owned());
        ProfileEdit {
            full_name: o(name),
            about: o(about),
            website: o(site),
            pronouns: o(pronouns),
        }
    }

    fn set_vcard(h: &mut Harness, edit: ProfileEdit) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::SetVCard { edit, reply }));
        answer
    }

    fn is_read(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Profile(Pending::VCardRead { .. }))
    }

    #[test]
    fn profile_reads_about_website_and_pronouns() {
        let mut h = Harness::new();
        let mut answer = get(&mut h, "bob@chord.localhost");
        h.sent_iqs();
        h.answer(is_nick, None);
        h.answer(
            is_vcard,
            Some(items(
                NODE_VCARD4,
                &format!(
                    "<vcard xmlns='{NS_VCARD4}'><fn><text>Robert</text></fn>
                     <note><text> Likes tea. </text></note>
                     <url><uri>https://bob.example</uri></url>
                     <pronouns><text>he/him</text></pronouns></vcard>"
                ),
            )),
        );
        let profile = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(profile.about.as_deref(), Some("Likes tea."));
        assert_eq!(profile.website.as_deref(), Some("https://bob.example"));
        assert_eq!(profile.pronouns.as_deref(), Some("he/him"));
    }

    #[test]
    fn set_profile_reads_first_and_keeps_the_other_properties() {
        let mut h = Harness::new();
        let mut answer = set_vcard(
            &mut h,
            edit(" Alice A ", "Hello", "https://a.example", "she/her"),
        );
        let sent = h.sent_iqs();
        let [Iq::Get { to, payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert_eq!(to.as_ref().unwrap().as_str(), ACCOUNT_BARE);
        assert_eq!(
            payload.get_child("items", ns::PUBSUB).unwrap().attr("node"),
            Some(NODE_VCARD4)
        );
        h.answer(
            is_read,
            Some(items(
                NODE_VCARD4,
                &format!(
                    "<vcard xmlns='{NS_VCARD4}'><fn><text>Old</text></fn>
                     <bday><date>1990-01-02</date></bday>
                     <note><text>Old note</text></note></vcard>"
                ),
            )),
        );
        let sent = h.sent_iqs();
        let [Iq::Set { to, payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert!(to.is_none());
        let publish = payload.get_child("publish", ns::PUBSUB).unwrap();
        assert_eq!(publish.attr("node"), Some(NODE_VCARD4));
        let vcard = publish
            .get_child("item", ns::PUBSUB)
            .unwrap()
            .get_child("vcard", NS_VCARD4)
            .unwrap();
        assert_eq!(text_of(vcard, "fn", "text").as_deref(), Some("Alice A"));
        assert_eq!(text_of(vcard, "note", "text").as_deref(), Some("Hello"));
        assert_eq!(
            text_of(vcard, "url", "uri").as_deref(),
            Some("https://a.example")
        );
        assert_eq!(
            text_of(vcard, "pronouns", "text").as_deref(),
            Some("she/her")
        );
        assert!(vcard.get_child("bday", NS_VCARD4).is_some());
        assert_eq!(vcard.children().filter(|c| c.name() == "note").count(), 1);
        assert_eq!(answer.try_recv().unwrap(), None);
        h.answer(is_publish, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn set_profile_starts_empty_when_there_is_no_item() {
        let mut h = Harness::new();
        let mut answer = set_vcard(&mut h, edit("", "About", "", ""));
        h.sent_iqs();
        h.respond(
            is_read,
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ItemNotFound,
                "en",
                "",
            )),
        );
        let sent = h.sent_iqs();
        let [Iq::Set { payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        let vcard = payload
            .get_child("publish", ns::PUBSUB)
            .unwrap()
            .get_child("item", ns::PUBSUB)
            .unwrap()
            .get_child("vcard", NS_VCARD4)
            .unwrap();
        // A vCard4 needs a name: the local part of the address fills in.
        assert_eq!(text_of(vcard, "fn", "text").as_deref(), Some("alice"));
        assert_eq!(text_of(vcard, "note", "text").as_deref(), Some("About"));
        h.answer(is_publish, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn set_profile_with_no_fields_retracts_the_item() {
        let mut h = Harness::new();
        let mut answer = set_vcard(&mut h, ProfileEdit::default());
        h.sent_iqs();
        h.answer(
            is_read,
            Some(items(
                NODE_VCARD4,
                &format!("<vcard xmlns='{NS_VCARD4}'><fn><text>Old</text></fn></vcard>"),
            )),
        );
        let sent = h.sent_iqs();
        let [Iq::Set { payload, .. }] = &sent[..] else {
            panic!("{sent:?}")
        };
        let retract = payload.get_child("retract", ns::PUBSUB).unwrap();
        assert_eq!(retract.attr("node"), Some(NODE_VCARD4));
        h.answer(is_publish, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn set_profile_checks_its_input_and_never_writes_over_an_unread_item() {
        let mut h = Harness::new();
        for bad in [
            edit("", &"x".repeat(MAX_ABOUT + 1), "", ""),
            edit("", "", "ftp://a.example", ""),
            edit("", "", "https://a b", ""),
            edit("", "", "", &"x".repeat(MAX_PRONOUNS + 1)),
        ] {
            let mut answer = set_vcard(&mut h, bad);
            assert!(matches!(
                answer.try_recv().unwrap(),
                Some(Err(ClientError::Invalid(_)))
            ));
        }
        assert!(h.sent_iqs().is_empty());
        let mut answer = set_vcard(&mut h, edit("A", "", "", ""));
        h.sent_iqs();
        h.respond(
            is_read,
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
        assert!(h.sent_iqs().is_empty());
        let mut answer = set_vcard(&mut h, edit("A", "", "", ""));
        h.respond(is_read, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn own_devices_list_the_other_resources_and_drop_the_offline_ones() {
        use xmpp_parsers::presence::{Presence, Show, Type};
        let mut h = Harness::new();
        let from = |r: &str| Jid::new(&format!("{ACCOUNT_BARE}/{r}")).unwrap();
        let mut phone = Presence::new(Type::None).with_from(from("phone"));
        phone.show = Some(Show::Away);
        h.with_ctx(|ctx| {
            crate::features::roster::on_presence(ctx, &phone);
            crate::features::roster::on_presence(
                ctx,
                &Presence::new(Type::None).with_from(from("desk")),
            );
        });
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Devices { reply }));
        let list = answer.try_recv().unwrap().unwrap().unwrap();
        let names: Vec<_> = list.iter().map(|d| d.resource.as_str()).collect();
        assert_eq!(names, ["desk", "phone"]);
        assert_eq!(list[1].show.as_deref(), Some("away"));
        h.with_ctx(|ctx| {
            crate::features::roster::on_presence(
                ctx,
                &Presence::new(Type::Unavailable).with_from(from("phone")),
            );
        });
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Devices { reply }));
        let list = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].resource, "desk");
    }
}
