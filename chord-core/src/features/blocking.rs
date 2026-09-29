//! Blocking command (XEP-0191): block and unblock addresses on the server.
//!
//! The server holds the blocklist and drops the traffic. Chord keeps a copy in the
//! `blocked_jids` table, so the list and the `blocked` flags work offline. The copy comes
//! from the fetch at each new session and from the pushes of the server. The flags follow
//! the server: a block or unblock command changes the copy only after the server answers.

use std::collections::BTreeSet;

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::params;
use xmpp_parsers::blocking::{Block, BlocklistRequest, BlocklistResult, Unblock};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending};
use super::{error_reply, result_reply};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::store::Store;
use crate::views::ViewKey;
use crate::views::channel_list::ChannelScope;

/// The namespace of XEP-0191. xmpp-parsers 0.23 uses `ns::BLOCKING` (blocking.rs, line 16).
pub const NS_BLOCKING: &str = "urn:xmpp:blocking";

type Reply<T> = oneshot::Sender<Result<T, ClientError>>;

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The blocklist request of a new session.
    Fetch,
    Block {
        jid: BareJid,
        reply: Reply<()>,
    },
    Unblock {
        /// `None` unblocks every address.
        jid: Option<BareJid>,
        reply: Reply<()>,
    },
}

/// A command from the public API.
pub(crate) enum Command {
    Block { jid: BareJid, reply: Reply<()> },
    Unblock { jid: BareJid, reply: Reply<()> },
    UnblockAll { reply: Reply<()> },
    List { reply: Reply<Vec<BareJid>> },
}

impl ClientHandle {
    /// Block a contact or any other address (XEP-0191). The server then drops the
    /// messages and presence from it. The command waits for service discovery. It fails
    /// with `Unsupported` when the server lacks `urn:xmpp:blocking`. The core emits
    /// `ClientEvent::BlockListChanged` when the list changes.
    pub async fn block_contact(&self, jid: BareJid) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Blocking(Command::Block { jid, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Unblock one address. It waits for service discovery like `block_contact`.
    pub async fn unblock_contact(&self, jid: BareJid) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Blocking(Command::Unblock { jid, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Clear the whole blocklist. It waits for service discovery like `block_contact`.
    pub async fn unblock_all(&self) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Blocking(Command::UnblockAll { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// The blocked addresses, from the stored copy of the blocklist. Works offline.
    /// An entry with a resource shows as its bare address.
    pub async fn blocked_contacts(&self) -> Result<Vec<BareJid>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Blocking(Command::List { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// Discovery finished. Fetch the blocklist when the server supports it.
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    if !supported(ctx) {
        return;
    }
    // xmpp-parsers 0.23, blocking.rs lines 13-19: an empty `blocklist` element.
    let iq = Iq::from_get("", BlocklistRequest);
    ctx.request(iq, FeaturePending::Blocking(Pending::Fetch));
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let outcome = match response {
        IqResponse::Result(payload) => Ok(payload),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    match pending {
        Pending::Fetch => match outcome {
            // Lines 21-29: the result holds one `item` per blocked address.
            Ok(Some(payload)) => match BlocklistResult::try_from(payload) {
                Ok(list) => replace(ctx, &list.items),
                Err(e) => log::warn!("bad blocklist from the server: {e}"),
            },
            Ok(None) => log::warn!("the blocklist answer has no payload"),
            Err(e) => log::debug!("blocklist fetch failed: {e}"),
        },
        Pending::Block { jid, reply } => {
            let outcome = outcome.map(|_| {
                add(ctx, &[Jid::from(jid)]);
            });
            let _ = reply.send(outcome);
        }
        Pending::Unblock { jid, reply } => {
            let outcome = outcome.map(|_| {
                match jid {
                    Some(jid) => remove(ctx, &[Jid::from(jid)]),
                    None => clear(ctx),
                };
            });
            let _ = reply.send(outcome);
        }
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::List { reply } => {
            let _ = reply.send(list(ctx.store, ctx.account_id));
        }
        Command::Block { jid, reply } => {
            if !supported(ctx) {
                let _ = reply.send(Err(unsupported()));
                return;
            }
            // Lines 33-41: one `item` with a `jid` per address.
            let items = vec![Jid::from(jid.clone())];
            let iq = Iq::from_set("", Block { items });
            ctx.request(iq, FeaturePending::Blocking(Pending::Block { jid, reply }));
        }
        Command::Unblock { jid, reply } => {
            if !supported(ctx) {
                let _ = reply.send(Err(unsupported()));
                return;
            }
            let items = vec![Jid::from(jid.clone())];
            let iq = Iq::from_set("", Unblock { items });
            let jid = Some(jid);
            ctx.request(
                iq,
                FeaturePending::Blocking(Pending::Unblock { jid, reply }),
            );
        }
        Command::UnblockAll { reply } => {
            if !supported(ctx) {
                let _ = reply.send(Err(unsupported()));
                return;
            }
            // Lines 45-54: an `unblock` with no item clears the list.
            let iq = Iq::from_set("", Unblock { items: Vec::new() });
            let pending = Pending::Unblock { jid: None, reply };
            ctx.request(iq, FeaturePending::Blocking(pending));
        }
    }
}

/// A command while no session is up. `features::on_command_offline` answers `List` with
/// `list` before it calls this function.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Block { reply, .. }
        | Command::Unblock { reply, .. }
        | Command::UnblockAll { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::List { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// A blocklist push (XEP-0191, 3.4 and 3.6). Returns true if `iq` is one.
pub(crate) fn on_iq(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Set { payload, from, .. } = iq else {
        return false;
    };
    let block = payload.is("block", NS_BLOCKING);
    if !block && !payload.is("unblock", NS_BLOCKING) {
        return false;
    }
    // The server sends a push from our bare JID or from its own domain. Nobody else may.
    if let Some(from) = from
        && from.to_bare() != *ctx.account
        && from.as_str() != ctx.account.domain().as_str()
    {
        log::warn!("ignored a blocklist push from {from}");
        return false;
    }
    if block {
        match Block::try_from(payload.clone()) {
            Ok(push) => {
                add(ctx, &push.items);
            }
            Err(_) => return bad_push(ctx, iq),
        }
    } else {
        match Unblock::try_from(payload.clone()) {
            // XEP-0191, 3.6: an `unblock` with no item clears the list.
            Ok(push) if push.items.is_empty() => {
                clear(ctx);
            }
            Ok(push) => {
                remove(ctx, &push.items);
            }
            Err(_) => return bad_push(ctx, iq),
        }
    }
    ctx.send(result_reply(iq, None));
    true
}

fn bad_push(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let error = StanzaError::new(
        ErrorType::Modify,
        DefinedCondition::BadRequest,
        "en",
        "Bad blocklist push",
    );
    ctx.send(error_reply(iq, error));
    true
}

/// The blocked addresses of the account, as bare JIDs. For `on_command_offline` too.
pub(crate) fn list(store: &Store, account_id: i64) -> Result<Vec<BareJid>, ClientError> {
    let store_error = |e: rusqlite::Error| ClientError::Invalid(format!("store: {e}"));
    let mut stmt = store
        .conn()
        .prepare("SELECT jid FROM blocked_jids WHERE account_id = ?1 ORDER BY jid")
        .map_err(store_error)?;
    let rows = stmt
        .query_map([account_id], |row| row.get::<_, String>(0))
        .map_err(store_error)?;
    let mut out = BTreeSet::new();
    for row in rows {
        if let Ok(jid) = Jid::new(&row.map_err(store_error)?) {
            out.insert(jid.to_bare());
        }
    }
    Ok(out.into_iter().collect())
}

/// Whether the store holds a block entry for `jid`, a bare JID as text.
pub(crate) fn is_blocked(store: &Store, account_id: i64, jid: &str) -> bool {
    store
        .conn()
        .query_row(
            "SELECT 1 FROM blocked_jids WHERE account_id = ?1 AND jid = ?2",
            params![account_id, jid],
            |_| Ok(()),
        )
        .is_ok()
}

fn supported(ctx: &Ctx<'_>) -> bool {
    ctx.state.disco.server_has(NS_BLOCKING)
}

fn unsupported() -> ClientError {
    ClientError::Unsupported("the server has no blocking command support".into())
}

/// Store new entries. Returns true if the list changed.
fn add(ctx: &mut Ctx<'_>, items: &[Jid]) -> bool {
    let mut changed = false;
    for item in items {
        match ctx.store.conn().execute(
            "INSERT OR IGNORE INTO blocked_jids (account_id, jid) VALUES (?1, ?2)",
            params![ctx.account_id, item.as_str()],
        ) {
            Ok(n) => changed |= n > 0,
            Err(e) => ctx.store_error("store a blocked address", e),
        }
    }
    finish(ctx, changed)
}

/// Delete entries. A bare JID also removes the entries with a resource.
fn remove(ctx: &mut Ctx<'_>, items: &[Jid]) -> bool {
    let mut changed = false;
    for item in items {
        let prefix = format!("{}/", item.to_bare());
        let result = if item.resource().is_some() {
            ctx.store.conn().execute(
                "DELETE FROM blocked_jids WHERE account_id = ?1 AND jid = ?2",
                params![ctx.account_id, item.as_str()],
            )
        } else {
            ctx.store.conn().execute(
                "DELETE FROM blocked_jids WHERE account_id = ?1
                 AND (jid = ?2 OR substr(jid, 1, length(?3)) = ?3)",
                params![ctx.account_id, item.as_str(), prefix],
            )
        };
        match result {
            Ok(n) => changed |= n > 0,
            Err(e) => ctx.store_error("delete a blocked address", e),
        }
    }
    finish(ctx, changed)
}

fn clear(ctx: &mut Ctx<'_>) -> bool {
    let changed = match ctx.store.conn().execute(
        "DELETE FROM blocked_jids WHERE account_id = ?1",
        [ctx.account_id],
    ) {
        Ok(n) => n > 0,
        Err(e) => {
            ctx.store_error("clear the blocked addresses", e);
            false
        }
    };
    finish(ctx, changed)
}

/// Make the stored list equal to the list from the server.
fn replace(ctx: &mut Ctx<'_>, items: &[Jid]) {
    let before = list(ctx.store, ctx.account_id).unwrap_or_default();
    let conn = ctx.store.conn();
    let result = conn
        .execute(
            "DELETE FROM blocked_jids WHERE account_id = ?1",
            [ctx.account_id],
        )
        .and_then(|_| {
            for item in items {
                conn.execute(
                    "INSERT OR IGNORE INTO blocked_jids (account_id, jid) VALUES (?1, ?2)",
                    params![ctx.account_id, item.as_str()],
                )?;
            }
            Ok(())
        });
    if let Err(e) = result {
        ctx.store_error("store the blocklist", e);
        return;
    }
    let after = list(ctx.store, ctx.account_id).unwrap_or_default();
    finish(ctx, before != after);
}

/// A change to the list moves the `blocked` flags. Tell the views and the frontends.
fn finish(ctx: &mut Ctx<'_>, changed: bool) -> bool {
    if changed {
        ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
        ctx.emit(ClientEvent::BlockListChanged);
    }
    changed
}

fn describe(error: &StanzaError) -> String {
    let text = error.texts.values().next().map(String::as_str);
    match text {
        Some(text) => format!("{:?}: {text}", error.defined_condition),
        None => format!("{:?}", error.defined_condition),
    }
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::minidom::Element;

    use super::*;
    use crate::features::testing::{ACCOUNT, Harness};
    use crate::features::{Effect, on_command, on_command_offline};

    type Answer = oneshot::Receiver<Result<(), ClientError>>;

    fn bare(s: &str) -> BareJid {
        BareJid::new(s).unwrap()
    }

    fn jid(s: &str) -> Jid {
        Jid::new(s).unwrap()
    }

    fn harness(supported: bool) -> Harness {
        let mut h = Harness::new();
        let mut info = crate::features::disco::info(None);
        if supported {
            info.features.insert(NS_BLOCKING.into());
        }
        h.state.disco.server = Some(info);
        h.state.disco.complete = true;
        h
    }

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn events(h: &mut Harness) -> Vec<ClientEvent> {
        std::mem::take(&mut h.effects)
            .into_iter()
            .filter_map(|e| match e {
                Effect::Emit(ev) => Some(ev),
                _ => None,
            })
            .collect()
    }

    fn stored(h: &Harness) -> Vec<String> {
        list(&h.store, h.account_id)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    fn push(h: &mut Harness, from: Option<&str>, xml: &str) -> Vec<Iq> {
        let iq = Iq::Set {
            from: from.map(jid),
            to: None,
            id: "p1".into(),
            payload: el(xml),
        };
        assert!(h.with_ctx(|ctx| on_iq(ctx, &iq)));
        h.sent_iqs()
    }

    fn respond(h: &mut Harness, response: IqResponse) {
        h.respond(|p| matches!(p, FeaturePending::Blocking(_)), response);
    }

    fn command(h: &mut Harness, make: impl FnOnce(Reply<()>) -> Command) -> Answer {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, FeatureCommand::Blocking(make(reply))));
        answer
    }

    fn block(h: &mut Harness, address: &str) -> Answer {
        command(h, |reply| Command::Block {
            jid: bare(address),
            reply,
        })
    }

    #[test]
    fn fetch_at_connect_fills_the_cache() {
        let mut h = harness(true);
        h.with_ctx(on_services_ready);
        let iqs = h.sent_iqs();
        let Iq::Get { payload, .. } = &iqs[0] else {
            panic!("not a get");
        };
        assert!(payload.is("blocklist", NS_BLOCKING));
        assert_eq!(payload.children().count(), 0);

        let answer = el("<blocklist xmlns='urn:xmpp:blocking'>\
             <item jid='spam@example.org'/><item jid='evil.example'/></blocklist>");
        h.answer(
            |p| matches!(p, FeaturePending::Blocking(Pending::Fetch)),
            Some(answer),
        );
        assert_eq!(stored(&h), ["evil.example", "spam@example.org"]);
        assert!(events(&mut h).contains(&ClientEvent::BlockListChanged));
        let home = ViewKey::ChannelList(ChannelScope::Home);
        assert!(h.take_dirty().contains(&home));
    }

    #[test]
    fn fetch_replaces_the_old_cache() {
        let mut h = harness(true);
        h.with_ctx(|ctx| add(ctx, &[jid("old@example.org")]));
        h.with_ctx(on_services_ready);
        let answer =
            el("<blocklist xmlns='urn:xmpp:blocking'><item jid='new@example.org'/></blocklist>");
        h.answer(|p| matches!(p, FeaturePending::Blocking(_)), Some(answer));
        assert_eq!(stored(&h), ["new@example.org"]);
    }

    #[test]
    fn no_fetch_without_support() {
        let mut h = harness(false);
        h.with_ctx(on_services_ready);
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn block_sends_the_item_and_stores_after_the_result() {
        let mut h = harness(true);
        let mut answer = block(&mut h, "spam@example.org");
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert!(payload.is("block", NS_BLOCKING));
        let item = payload.children().next().unwrap();
        assert_eq!(item.name(), "item");
        assert_eq!(item.attr("jid"), Some("spam@example.org"));
        assert!(stored(&h).is_empty());
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        assert_eq!(stored(&h), ["spam@example.org"]);
        assert_eq!(events(&mut h), [ClientEvent::BlockListChanged]);
    }

    #[test]
    fn a_push_after_the_result_does_not_repeat_the_event() {
        let mut h = harness(true);
        let _answer = block(&mut h, "spam@example.org");
        respond(&mut h, IqResponse::Result(None));
        events(&mut h);
        push(
            &mut h,
            None,
            "<block xmlns='urn:xmpp:blocking'><item jid='spam@example.org'/></block>",
        );
        assert!(events(&mut h).is_empty());
    }

    #[test]
    fn block_error_stores_nothing() {
        let mut h = harness(true);
        let mut answer = block(&mut h, "spam@example.org");
        let error = StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::NotAcceptable,
            "en",
            "no",
        );
        respond(&mut h, IqResponse::Error(error));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Server(_))
        ));
        assert!(stored(&h).is_empty());
    }

    #[test]
    fn unblock_and_unblock_all_shapes() {
        let mut h = harness(true);
        for address in ["a@example.org", "b@example.org"] {
            h.with_ctx(|ctx| add(ctx, &[jid(address)]));
        }
        let mut answer = command(&mut h, |reply| Command::Unblock {
            jid: bare("a@example.org"),
            reply,
        });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert!(payload.is("unblock", NS_BLOCKING));
        let item = payload.children().next().unwrap();
        assert_eq!(item.attr("jid"), Some("a@example.org"));
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        assert_eq!(stored(&h), ["b@example.org"]);

        let mut answer = command(&mut h, |reply| Command::UnblockAll { reply });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set");
        };
        assert!(payload.is("unblock", NS_BLOCKING));
        assert_eq!(payload.children().count(), 0);
        respond(&mut h, IqResponse::Result(None));
        assert!(answer.try_recv().unwrap().unwrap().is_ok());
        assert!(stored(&h).is_empty());
    }

    #[test]
    fn push_block_and_unblock_answer_with_a_result() {
        let mut h = harness(true);
        let sent = push(
            &mut h,
            Some(ACCOUNT),
            "<block xmlns='urn:xmpp:blocking'><item jid='a@example.org'/>\
             <item jid='b@example.org'/></block>",
        );
        assert!(matches!(&sent[0], Iq::Result { id, .. } if id == "p1"));
        assert_eq!(stored(&h), ["a@example.org", "b@example.org"]);
        assert!(events(&mut h).contains(&ClientEvent::BlockListChanged));

        let sent = push(
            &mut h,
            None,
            "<unblock xmlns='urn:xmpp:blocking'><item jid='a@example.org'/></unblock>",
        );
        assert!(matches!(&sent[0], Iq::Result { .. }));
        assert_eq!(stored(&h), ["b@example.org"]);
        assert!(events(&mut h).contains(&ClientEvent::BlockListChanged));
    }

    #[test]
    fn push_unblock_without_items_clears_the_list() {
        let mut h = harness(true);
        h.with_ctx(|ctx| add(ctx, &[jid("a@example.org")]));
        let sent = push(
            &mut h,
            Some("chord.localhost"),
            "<unblock xmlns='urn:xmpp:blocking'/>",
        );
        assert!(matches!(&sent[0], Iq::Result { .. }));
        assert!(stored(&h).is_empty());
    }

    #[test]
    fn push_from_a_stranger_is_not_handled() {
        let mut h = harness(true);
        let iq = Iq::Set {
            from: Some(jid("mallory@example.org")),
            to: None,
            id: "p2".into(),
            payload: el("<block xmlns='urn:xmpp:blocking'><item jid='a@example.org'/></block>"),
        };
        assert!(!h.with_ctx(|ctx| on_iq(ctx, &iq)));
        assert!(stored(&h).is_empty());
    }

    #[test]
    fn unsupported_server_sends_nothing() {
        let mut h = harness(false);
        let mut answer = block(&mut h, "a@example.org");
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Unsupported(_))
        ));
        let mut answer = command(&mut h, |reply| Command::UnblockAll { reply });
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::Unsupported(_))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn commands_wait_for_service_discovery() {
        let mut h = harness(true);
        h.state.disco.complete = false;
        let _answer = block(&mut h, "a@example.org");
        assert!(h.sent_iqs().is_empty());
        h.state.disco.complete = true;
        h.with_ctx(crate::features::on_services_ready);
        let iqs = h.sent_iqs();
        assert!(iqs.iter().any(|iq| matches!(
            iq,
            Iq::Set { payload, .. } if payload.is("block", NS_BLOCKING)
        )));
    }

    #[test]
    fn list_works_offline() {
        let mut h = harness(true);
        h.with_ctx(|ctx| add(ctx, &[jid("a@example.org/phone")]));
        assert!(is_blocked(&h.store, h.account_id, "a@example.org/phone"));
        let (reply, mut answer) = oneshot::channel();
        on_command_offline(
            &h.store,
            h.account_id,
            FeatureCommand::Blocking(Command::List { reply }),
        );
        assert_eq!(
            answer.try_recv().unwrap().unwrap().unwrap(),
            [bare("a@example.org")]
        );
        let (reply, mut answer) = oneshot::channel();
        let block = Command::Block {
            jid: bare("b@example.org"),
            reply,
        };
        on_command_offline(&h.store, h.account_id, FeatureCommand::Blocking(block));
        assert!(matches!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::NotConnected)
        ));
    }
}
