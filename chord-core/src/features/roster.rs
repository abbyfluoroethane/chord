//! Roster (RFC 6121): the contact list, roster pushes, and contact presence.
//!
//! - Gets the roster at each new session, with roster versioning (RFC 6121, 2.6).
//! - Applies roster pushes. A push counts only from our own account (RFC 6121, 2.1.6).
//! - Stores the presence of contacts, one row per resource.
//! - Reports a subscription request with `ClientEvent::SubscriptionRequest`.
//! - Pre-approves a subscription (RFC 6121, 3.4) with `ClientHandle::preapprove_subscription`.
//!   The stream feature `sub` is not visible here, so the command sends the `subscribed`
//!   presence and then a ping as a barrier. The server sends the roster push before it
//!   answers the ping. If the push did not set `approved`, the server does not support
//!   pre-approval, and the command fails with `Unsupported`.
//! - The `contacts.ask` column holds two bits: 1 for a pending request of ours, 2 for a
//!   pre-approval. It needs no new column.

use std::collections::{HashMap, HashSet};

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{Connection, OptionalExtension, params};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::ns;
use xmpp_parsers::ping::Ping;
use xmpp_parsers::presence::{Presence, Show, Type};
use xmpp_parsers::roster::{Ask, Group, Item, Roster, Subscription as WireSubscription};
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use super::{
    Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, error_reply, result_reply,
};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::views::{ChannelScope, ViewKey};

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// Contacts that asked to see our presence, and that we did not answer yet.
    requests: HashSet<BareJid>,
    /// Pre-approvals that wait for their barrier ping, with the callers to answer.
    preapprovals: HashMap<BareJid, Vec<Reply>>,
    /// The server offers subscription pre-approval (RFC 6121, 3.4).
    pub pre_approval: bool,
}

/// The stream feature for subscription pre-approval (RFC 6121, 3.4).
pub const NS_PRE_APPROVAL: &str = "urn:xmpp:features:pre-approval";

type Reply = oneshot::Sender<Result<(), ClientError>>;

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The roster get at the start of the session.
    Get,
    /// A roster set that adds or changes a contact. Then we ask for the subscription.
    Add { jid: BareJid, reply: Reply },
    /// A roster set that removes a contact.
    Remove { reply: Reply },
    /// The ping that follows a pre-approval. See the module comment.
    Preapprove { jid: BareJid },
}

/// A command from the public API.
pub(crate) enum Command {
    Add {
        jid: BareJid,
        name: Option<String>,
        reply: Reply,
    },
    Remove {
        jid: BareJid,
        reply: Reply,
    },
    Approve {
        jid: BareJid,
        reply: Reply,
    },
    Deny {
        jid: BareJid,
        reply: Reply,
    },
    Preapprove {
        jid: BareJid,
        reply: Reply,
    },
    Contacts {
        reply: oneshot::Sender<Result<Vec<Contact>, ClientError>>,
    },
}

/// Our subscription state with a contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub enum Subscription {
    /// Neither side sees the presence of the other.
    None,
    /// We see the presence of the contact.
    To,
    /// The contact sees our presence.
    From,
    /// Both sides see the presence of the other.
    Both,
}

impl Subscription {
    fn parse(s: &str) -> Self {
        match s {
            "to" => Self::To,
            "from" => Self::From,
            "both" => Self::Both,
            _ => Self::None,
        }
    }
}

/// A contact of the roster.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "camelCase")
)]
pub struct Contact {
    pub jid: BareJid,
    pub name: Option<String>,
    pub subscription: Subscription,
    /// We asked for a subscription and wait for the answer.
    pub ask: bool,
    pub groups: Vec<String>,
    /// We pre-approved the subscription request of this contact (RFC 6121, 3.4). The
    /// server accepts the request for us when it comes.
    pub approved: bool,
    /// The blocklist of the account (XEP-0191) holds this contact.
    pub blocked: bool,
}

impl ClientHandle {
    /// Add a contact to the roster and ask to see its presence. Answers after the server
    /// accepts the roster change.
    pub async fn add_contact(&self, jid: BareJid, name: Option<String>) -> Result<(), ClientError> {
        self.roster_call(|reply| Command::Add { jid, name, reply })
            .await
    }

    /// Remove a contact from the roster. The server also ends both subscriptions.
    pub async fn remove_contact(&self, jid: BareJid) -> Result<(), ClientError> {
        self.roster_call(|reply| Command::Remove { jid, reply })
            .await
    }

    /// Let a contact see our presence. Answer to `ClientEvent::SubscriptionRequest`.
    pub async fn approve_subscription(&self, jid: BareJid) -> Result<(), ClientError> {
        self.roster_call(|reply| Command::Approve { jid, reply })
            .await
    }

    /// Refuse a subscription request, or stop a contact from seeing our presence.
    pub async fn deny_subscription(&self, jid: BareJid) -> Result<(), ClientError> {
        self.roster_call(|reply| Command::Deny { jid, reply }).await
    }

    /// Accept a subscription request of `jid` before it arrives (RFC 6121, 3.4). The
    /// server creates a roster item with `approved` set, and accepts the request when it
    /// comes. If `jid` sent a request already, this is the same as `approve_subscription`.
    /// Fails with `Unsupported` when the server does not support pre-approval.
    /// `remove_contact` cancels a pre-approval. A server may keep the flag after
    /// `deny_subscription`.
    pub async fn preapprove_subscription(&self, jid: BareJid) -> Result<(), ClientError> {
        self.roster_call(|reply| Command::Preapprove { jid, reply })
            .await
    }

    /// The contacts of the roster, sorted by name. Needs a session.
    pub async fn contacts(&self) -> Result<Vec<Contact>, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Roster(Command::Contacts { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    async fn roster_call(&self, command: impl FnOnce(Reply) -> Command) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Roster(command(reply)))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// Ask for the roster. The version of the stored roster lets the server send only a
/// change, or nothing.
pub(crate) fn on_connected(ctx: &mut Ctx<'_>, stream_features: &[String]) {
    ctx.state.roster.pre_approval = stream_features.iter().any(|f| f == NS_PRE_APPROVAL);
    let version = match stored_version(ctx.store.conn(), ctx.account_id) {
        Ok(version) => version,
        Err(e) => {
            ctx.store_error("read the roster version", e);
            None
        }
    };
    // An empty `ver` tells the server that we support versioning, but hold no roster.
    let query = Roster {
        ver: Some(version.unwrap_or_default()),
        items: vec![],
    };
    ctx.request(
        Iq::from_get("", query),
        FeaturePending::Roster(Pending::Get),
    );
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Get => on_roster_result(ctx, response),
        Pending::Add { jid, reply } => match response {
            IqResponse::Result(_) => {
                ctx.send(Presence::subscribe().with_to(jid));
                let _ = reply.send(Ok(()));
            }
            other => {
                let _ = reply.send(Err(failure(other)));
            }
        },
        Pending::Remove { reply } => {
            let result = match response {
                IqResponse::Result(_) => Ok(()),
                other => Err(failure(other)),
            };
            let _ = reply.send(result);
        }
        Pending::Preapprove { jid } => on_preapprove_barrier(ctx, jid, response),
    }
}

/// The ping came back, so the roster push of the server, if any, came first.
fn on_preapprove_barrier(ctx: &mut Ctx<'_>, jid: BareJid, response: IqResponse) {
    let replies = ctx
        .state
        .roster
        .preapprovals
        .remove(&jid)
        .unwrap_or_default();
    let result = if matches!(response, IqResponse::Lost) {
        Err(ClientError::NotConnected)
    } else {
        match read_contact(ctx.store.conn(), ctx.account_id, &jid) {
            Ok(Some(c)) if is_preapproved(&c) => Ok(()),
            _ => Err(ClientError::Unsupported(
                "the server did not pre-approve the subscription".into(),
            )),
        }
    };
    for reply in replies {
        let _ = reply.send(result.clone());
    }
}

/// True if the contact sees our presence already, or the server will accept its request.
fn is_preapproved(contact: &Contact) -> bool {
    contact.approved
        || matches!(
            contact.subscription,
            Subscription::From | Subscription::Both
        )
}

fn failure(response: IqResponse) -> ClientError {
    match response {
        IqResponse::Error(e) => ClientError::Server(format!("{:?}", e.defined_condition)),
        _ => ClientError::NotConnected,
    }
}

fn on_roster_result(ctx: &mut Ctx<'_>, response: IqResponse) {
    let payload = match response {
        // No payload means that our stored roster is current.
        IqResponse::Result(None) | IqResponse::Lost => return,
        IqResponse::Result(Some(payload)) => payload,
        IqResponse::Error(e) => {
            log::warn!("roster get failed: {:?}", e.defined_condition);
            return;
        }
    };
    let roster = match Roster::try_from(payload) {
        Ok(roster) => roster,
        Err(e) => {
            log::warn!("bad roster result: {e}");
            return;
        }
    };
    let mut touched = stored_jids(ctx.store.conn(), ctx.account_id).unwrap_or_default();
    if let Err(e) = replace_all(ctx.store.conn(), ctx.account_id, &roster) {
        ctx.store_error("store the roster", e);
        return;
    }
    touched.extend(roster.items.into_iter().map(|item| item.jid));
    for jid in touched {
        mark(ctx, jid);
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Add { jid, name, reply } => {
            // A roster set replaces the groups, so keep the ones that we know.
            let known = read_contact(ctx.store.conn(), ctx.account_id, &jid)
                .ok()
                .flatten();
            let name = name.or_else(|| known.as_ref().and_then(|c| c.name.clone()));
            let groups = known.map(|c| c.groups).unwrap_or_default();
            let item = Item {
                jid: jid.clone(),
                name,
                subscription: WireSubscription::None,
                ask: Ask::None,
                groups: groups.into_iter().map(Group).collect(),
                approved: None,
            };
            let iq = Iq::from_set("", roster_set(item));
            ctx.request(iq, FeaturePending::Roster(Pending::Add { jid, reply }));
        }
        Command::Remove { jid, reply } => {
            let item = Item {
                jid,
                name: None,
                subscription: WireSubscription::Remove,
                ask: Ask::None,
                groups: vec![],
                approved: None,
            };
            let iq = Iq::from_set("", roster_set(item));
            ctx.request(iq, FeaturePending::Roster(Pending::Remove { reply }));
        }
        Command::Approve { jid, reply } => {
            ctx.state.roster.requests.remove(&jid);
            ctx.send(Presence::subscribed().with_to(jid));
            let _ = reply.send(Ok(()));
        }
        Command::Deny { jid, reply } => {
            ctx.state.roster.requests.remove(&jid);
            ctx.send(Presence::new(Type::Unsubscribed).with_to(jid));
            let _ = reply.send(Ok(()));
        }
        Command::Preapprove { jid, reply } => preapprove(ctx, jid, reply),
        Command::Contacts { reply } => {
            let result = list_contacts(ctx.store.conn(), ctx.account_id)
                .map_err(|e| ClientError::Invalid(format!("store: {e}")));
            let _ = reply.send(result);
        }
    }
}

fn preapprove(ctx: &mut Ctx<'_>, jid: BareJid, reply: Reply) {
    if jid == *ctx.account {
        let _ = reply.send(Err(ClientError::Invalid(
            "we cannot pre-approve our own account".into(),
        )));
        return;
    }
    // A request waits: the presence is a normal approval.
    if ctx.state.roster.requests.remove(&jid) {
        ctx.send(Presence::subscribed().with_to(jid));
        let _ = reply.send(Ok(()));
        return;
    }
    // RFC 6121, 3.4: a client must not pre-approve when the server does not offer it.
    if !ctx.state.roster.pre_approval {
        let _ = reply.send(Err(ClientError::Unsupported(
            "the server does not offer subscription pre-approval".into(),
        )));
        return;
    }
    let known = read_contact(ctx.store.conn(), ctx.account_id, &jid)
        .ok()
        .flatten();
    if known.as_ref().is_some_and(is_preapproved) {
        let _ = reply.send(Ok(()));
        return;
    }
    let waiting = ctx
        .state
        .roster
        .preapprovals
        .entry(jid.clone())
        .or_default();
    waiting.push(reply);
    if waiting.len() == 1 {
        ctx.send(Presence::subscribed().with_to(jid.clone()));
        ctx.request(
            Iq::from_get("", Ping),
            FeaturePending::Roster(Pending::Preapprove { jid }),
        );
    }
}

fn roster_set(item: Item) -> Roster {
    Roster {
        ver: None,
        items: vec![item],
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Add { reply, .. }
        | Command::Remove { reply, .. }
        | Command::Approve { reply, .. }
        | Command::Deny { reply, .. }
        | Command::Preapprove { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
        Command::Contacts { reply } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// A presence from a contact.
pub(crate) fn on_presence(ctx: &mut Ctx<'_>, presence: &Presence) {
    let Some(from) = &presence.from else {
        return;
    };
    let bare = from.to_bare();
    // Our other resources are not contacts.
    if bare == *ctx.account {
        return;
    }
    match presence.type_ {
        Type::None => {
            store_presence(ctx, from, presence);
            mark(ctx, bare);
        }
        Type::Unavailable => {
            remove_presence(ctx, from);
            mark(ctx, bare);
        }
        Type::Subscribe => {
            if ctx.state.roster.requests.insert(bare.clone()) {
                ctx.emit(ClientEvent::SubscriptionRequest(bare));
            }
        }
        // The roster push that follows carries the new state. We get no more presence
        // from this contact, so its presence goes.
        Type::Unsubscribed => {
            remove_presence(ctx, &Jid::from(bare.clone()));
            mark(ctx, bare);
        }
        Type::Unsubscribe => {
            ctx.state.roster.requests.remove(&bare);
        }
        Type::Subscribed => mark(ctx, bare),
        Type::Probe | Type::Error => {
            log::debug!("ignored {:?} presence from {from}", presence.type_);
        }
    }
}

fn store_presence(ctx: &mut Ctx<'_>, from: &Jid, presence: &Presence) {
    let show = presence.show.as_ref().map(show_str);
    let status = presence
        .statuses
        .values()
        .next()
        .filter(|s| !s.is_empty())
        .cloned();
    let result = ctx.store.conn().execute(
        "INSERT INTO presences (account_id, jid, bare, show, status, priority)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account_id, jid) DO UPDATE SET
             bare = excluded.bare, show = excluded.show,
             status = excluded.status, priority = excluded.priority",
        params![
            ctx.account_id,
            from.as_str(),
            from.to_bare().as_str(),
            show,
            status,
            presence.priority.0
        ],
    );
    if let Err(e) = result {
        ctx.store_error("store a presence", e);
    }
}

fn show_str(show: &Show) -> &'static str {
    match show {
        Show::Away => "away",
        Show::Chat => "chat",
        Show::Dnd => "dnd",
        Show::Xa => "xa",
    }
}

/// Delete the presence of one resource, or of all resources for a bare JID.
fn remove_presence(ctx: &mut Ctx<'_>, from: &Jid) {
    let column = if from.is_bare() { "bare" } else { "jid" };
    let result = ctx.store.conn().execute(
        &format!("DELETE FROM presences WHERE account_id = ?1 AND {column} = ?2"),
        params![ctx.account_id, from.as_str()],
    );
    if let Err(e) = result {
        ctx.store_error("delete a presence", e);
    }
}

/// Mark the views that show a contact: the home list, the members, and the timeline.
fn mark(ctx: &mut Ctx<'_>, peer: BareJid) {
    ctx.changed(ViewKey::ChannelList(ChannelScope::Home));
    ctx.changed(ViewKey::MemberList(peer.clone()));
    ctx.changed(ViewKey::Timeline(peer));
}

/// An IQ get or set to us. Returns true if it is a roster push. A push from anybody but
/// our own account is not ours: it returns false, and the caller answers with an error.
pub(crate) fn on_iq(ctx: &mut Ctx<'_>, iq: &Iq) -> bool {
    let Iq::Set { payload, from, .. } = iq else {
        return false;
    };
    if !payload.is("query", ns::ROSTER) {
        return false;
    }
    if let Some(from) = from
        && from.as_str() != ctx.account.as_str()
    {
        log::warn!("ignored a roster push from {from}");
        return false;
    }
    let roster = match Roster::try_from(payload.clone()) {
        // RFC 6121, 2.1.6: a push has exactly one item.
        Ok(roster) if roster.items.len() == 1 => roster,
        _ => {
            let error = StanzaError::new(
                ErrorType::Modify,
                DefinedCondition::BadRequest,
                "en",
                "A roster push has exactly one item",
            );
            ctx.send(error_reply(iq, error));
            return true;
        }
    };
    apply_push(ctx, roster);
    ctx.send(result_reply(iq, None));
    true
}

fn apply_push(ctx: &mut Ctx<'_>, roster: Roster) {
    let conn = ctx.store.conn();
    let mut applied = true;
    let mut touched = Vec::new();
    for item in &roster.items {
        let result = if item.subscription == WireSubscription::Remove {
            delete_contact(conn, ctx.account_id, &item.jid)
        } else {
            upsert_contact(conn, ctx.account_id, item)
        };
        match result {
            Ok(()) => touched.push(item.jid.clone()),
            Err(e) => {
                applied = false;
                ctx.store_error("apply a roster push", e);
            }
        }
    }
    // Keep the new version only when the store holds the change.
    if applied
        && let Some(ver) = &roster.ver
        && let Err(e) = save_version(ctx.store.conn(), ctx.account_id, Some(ver))
    {
        ctx.store_error("store the roster version", e);
    }
    for jid in touched {
        mark(ctx, jid);
    }
}

// Store access.

fn stored_version(conn: &Connection, account_id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT version FROM roster_versions WHERE account_id = ?1",
        params![account_id],
        |row| row.get(0),
    )
    .optional()
}

fn save_version(conn: &Connection, account_id: i64, ver: Option<&str>) -> rusqlite::Result<()> {
    match ver {
        Some(ver) => conn.execute(
            "INSERT INTO roster_versions (account_id, version) VALUES (?1, ?2)
             ON CONFLICT (account_id) DO UPDATE SET version = excluded.version",
            params![account_id, ver],
        ),
        None => conn.execute(
            "DELETE FROM roster_versions WHERE account_id = ?1",
            params![account_id],
        ),
    }
    .map(|_| ())
}

fn stored_jids(conn: &Connection, account_id: i64) -> rusqlite::Result<HashSet<BareJid>> {
    let mut stmt = conn.prepare_cached("SELECT jid FROM contacts WHERE account_id = ?1")?;
    let rows = stmt.query_map(params![account_id], |row| row.get::<_, String>(0))?;
    Ok(rows
        .filter_map(|jid| jid.ok())
        .filter_map(|jid| BareJid::new(&jid).ok())
        .collect())
}

/// Replace the whole roster with a full result from the server.
fn replace_all(conn: &Connection, account_id: i64, roster: &Roster) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM contacts WHERE account_id = ?1",
        params![account_id],
    )?;
    for item in &roster.items {
        if item.subscription != WireSubscription::Remove {
            upsert_contact(&tx, account_id, item)?;
        }
    }
    save_version(&tx, account_id, roster.ver.as_deref())?;
    tx.commit()
}

fn upsert_contact(conn: &Connection, account_id: i64, item: &Item) -> rusqlite::Result<()> {
    let groups: Vec<&str> = item.groups.iter().map(|g| g.0.as_str()).collect();
    conn.execute(
        "INSERT INTO contacts (account_id, jid, name, subscription, ask, groups)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (account_id, jid) DO UPDATE SET
             name = excluded.name, subscription = excluded.subscription,
             ask = excluded.ask, groups = excluded.groups",
        params![
            account_id,
            item.jid.as_str(),
            item.name,
            subscription_str(&item.subscription),
            ASK_BIT * i64::from(item.ask == Ask::Subscribe)
                + APPROVED_BIT * i64::from(item.approved == Some(true)),
            groups_to_json(&groups),
        ],
    )
    .map(|_| ())
}

/// The bits of the `contacts.ask` column.
const ASK_BIT: i64 = 1;
const APPROVED_BIT: i64 = 2;

fn subscription_str(subscription: &WireSubscription) -> &'static str {
    match subscription {
        WireSubscription::From => "from",
        WireSubscription::To => "to",
        WireSubscription::Both => "both",
        WireSubscription::None | WireSubscription::Remove => "none",
    }
}

/// Delete a contact and the presence that we kept for it.
fn delete_contact(conn: &Connection, account_id: i64, jid: &BareJid) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM contacts WHERE account_id = ?1 AND jid = ?2",
        params![account_id, jid.as_str()],
    )?;
    conn.execute(
        "DELETE FROM presences WHERE account_id = ?1 AND bare = ?2",
        params![account_id, jid.as_str()],
    )
    .map(|_| ())
}

const CONTACT_COLUMNS: &str = "jid, name, subscription, ask, groups,
     EXISTS(SELECT 1 FROM blocked_jids b
            WHERE b.account_id = contacts.account_id AND b.jid = contacts.jid)";

fn contact_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<Contact>> {
    let jid: String = row.get(0)?;
    let subscription: String = row.get(2)?;
    let groups: String = row.get(4)?;
    let flags = row.get::<_, i64>(3).unwrap_or(0);
    Ok(BareJid::new(&jid).ok().map(|jid| Contact {
        jid,
        name: row.get(1).ok().flatten(),
        subscription: Subscription::parse(&subscription),
        ask: flags & ASK_BIT != 0,
        groups: groups_from_json(&groups),
        approved: flags & APPROVED_BIT != 0,
        blocked: row.get::<_, i64>(5).unwrap_or(0) != 0,
    }))
}

fn read_contact(
    conn: &Connection,
    account_id: i64,
    jid: &BareJid,
) -> rusqlite::Result<Option<Contact>> {
    let sql = format!("SELECT {CONTACT_COLUMNS} FROM contacts WHERE account_id = ?1 AND jid = ?2");
    let found = conn
        .query_row(&sql, params![account_id, jid.as_str()], contact_from_row)
        .optional()?;
    Ok(found.flatten())
}

pub(crate) fn list_contacts(conn: &Connection, account_id: i64) -> rusqlite::Result<Vec<Contact>> {
    let sql = format!(
        "SELECT {CONTACT_COLUMNS} FROM contacts WHERE account_id = ?1
         ORDER BY lower(coalesce(name, jid)), jid"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![account_id], contact_from_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.extend(row?);
    }
    Ok(out)
}

// A JSON array of strings, by hand. The `groups` column holds it.

fn groups_to_json(groups: &[&str]) -> String {
    let mut out = String::from("[");
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        for c in group.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

/// Parse what `groups_to_json` wrote. Bad input gives the strings that parsed so far.
fn groups_from_json(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = json.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        loop {
            match chars.next() {
                None => return out,
                Some('"') => break,
                Some('\\') => match chars.next() {
                    Some('n') => s.push('\n'),
                    Some('r') => s.push('\r'),
                    Some('t') => s.push('\t'),
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            s.push(c);
                        }
                    }
                    Some(other) => s.push(other),
                    None => return out,
                },
                Some(c) => s.push(c),
            }
        }
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use xmpp_parsers::minidom::Element;
    use xmpp_parsers::stanza::Stanza;

    const BOB: &str = "bob@chord.localhost";

    fn is_get(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Roster(Pending::Get))
    }

    fn is_add(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Roster(Pending::Add { .. }))
    }

    fn is_remove(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Roster(Pending::Remove { .. }))
    }

    fn el(xml: &str) -> Element {
        xml.parse().unwrap()
    }

    fn bare(s: &str) -> BareJid {
        BareJid::new(s).unwrap()
    }

    fn push(from: Option<&str>, xml: &str) -> Iq {
        let mut iq = Iq::Set {
            from: from.map(|f| Jid::new(f).unwrap()),
            to: None,
            id: "p1".into(),
            payload: el(xml),
        };
        if let Iq::Set { to, .. } = &mut iq {
            *to = Some(Jid::new("alice@chord.localhost/chord").unwrap());
        }
        iq
    }

    fn presence(from: &str, type_: Type) -> Presence {
        Presence::new(type_).with_from(Jid::new(from).unwrap())
    }

    fn contacts(h: &Harness) -> Vec<Contact> {
        list_contacts(h.store.conn(), h.account_id).unwrap()
    }

    fn connect_with(h: &mut Harness, roster_xml: &str) {
        h.with_ctx(|ctx| on_connected(ctx, &[]));
        h.answer(is_get, Some(el(roster_xml)));
    }

    const FULL: &str = "<query xmlns='jabber:iq:roster' ver='v1'>\
        <item jid='bob@chord.localhost' name='Bob' subscription='both'><group>Friends</group><group>A \"b\"</group></item>\
        <item jid='carol@chord.localhost' subscription='none' ask='subscribe'/></query>";

    #[test]
    fn get_asks_with_an_empty_ver_at_first() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_connected(ctx, &[]));
        let iqs = h.sent_iqs();
        let Iq::Get { payload, .. } = &iqs[0] else {
            panic!("{iqs:?}")
        };
        let roster = Roster::try_from(payload.clone()).unwrap();
        assert_eq!(roster.ver, Some(String::new()));
    }

    #[test]
    fn result_is_stored_with_its_version() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        let list = contacts(&h);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].jid, bare(BOB));
        assert_eq!(list[0].name.as_deref(), Some("Bob"));
        assert_eq!(list[0].subscription, Subscription::Both);
        assert_eq!(list[0].groups, vec!["Friends", "A \"b\""]);
        assert!(!list[0].ask);
        assert_eq!(list[1].subscription, Subscription::None);
        assert!(list[1].ask);
        assert_eq!(
            stored_version(h.store.conn(), h.account_id).unwrap(),
            Some("v1".into())
        );
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::ChannelList(ChannelScope::Home)));
        assert!(dirty.contains(&ViewKey::MemberList(bare(BOB))));
        assert!(dirty.contains(&ViewKey::Timeline(bare(BOB))));
    }

    #[test]
    fn next_session_sends_the_stored_version_and_an_empty_result_keeps_the_roster() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        h.sent_iqs();
        h.with_ctx(|ctx| on_connected(ctx, &[]));
        let iqs = h.sent_iqs();
        let Iq::Get { payload, .. } = &iqs[0] else {
            panic!("{iqs:?}")
        };
        assert_eq!(
            Roster::try_from(payload.clone()).unwrap().ver,
            Some("v1".into())
        );
        h.answer(is_get, None);
        assert_eq!(contacts(&h).len(), 2);
    }

    #[test]
    fn full_result_replaces_the_old_roster() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        h.take_dirty();
        connect_with(
            &mut h,
            "<query xmlns='jabber:iq:roster' ver='v2'>\
             <item jid='dave@chord.localhost' subscription='to'/></query>",
        );
        let list = contacts(&h);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].jid, bare("dave@chord.localhost"));
        assert_eq!(
            stored_version(h.store.conn(), h.account_id).unwrap(),
            Some("v2".into())
        );
        // The removed contact changes its views, too.
        assert!(h.take_dirty().contains(&ViewKey::MemberList(bare(BOB))));
    }

    #[test]
    fn roster_get_error_and_lost_keep_the_store() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        for response in [
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ServiceUnavailable,
                "en",
                "no",
            )),
            IqResponse::Lost,
        ] {
            h.with_ctx(|ctx| on_connected(ctx, &[]));
            h.respond(is_get, response);
            assert_eq!(contacts(&h).len(), 2);
        }
    }

    #[test]
    fn push_from_our_account_is_applied_and_answered() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        h.take_dirty();
        h.sent_iqs();
        let xml = "<query xmlns='jabber:iq:roster' ver='v3'>\
            <item jid='erin@chord.localhost' name='Erin' subscription='from'/></query>";
        for from in [None, Some("alice@chord.localhost")] {
            let iq = push(from, xml);
            assert!(h.with_ctx(|ctx| on_iq(ctx, &iq)));
        }
        let list = contacts(&h);
        assert_eq!(list.len(), 3);
        let erin = list.iter().find(|c| c.jid == bare("erin@chord.localhost"));
        assert_eq!(erin.unwrap().subscription, Subscription::From);
        assert_eq!(
            stored_version(h.store.conn(), h.account_id).unwrap(),
            Some("v3".into())
        );
        let sent = h.sent_iqs();
        assert_eq!(sent.len(), 2);
        assert!(
            sent.iter()
                .all(|iq| matches!(iq, Iq::Result { id, .. } if id == "p1"))
        );
        assert!(
            h.take_dirty()
                .contains(&ViewKey::MemberList(bare("erin@chord.localhost")))
        );
    }

    #[test]
    fn push_with_remove_deletes_the_contact_and_its_presence() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        h.with_ctx(|ctx| on_presence(ctx, &presence("bob@chord.localhost/phone", Type::None)));
        let iq = push(
            None,
            "<query xmlns='jabber:iq:roster' ver='v4'>\
             <item jid='bob@chord.localhost' subscription='remove'/></query>",
        );
        assert!(h.with_ctx(|ctx| on_iq(ctx, &iq)));
        assert_eq!(contacts(&h).len(), 1);
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT count(*) FROM presences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn push_from_another_jid_is_rejected() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        h.take_dirty();
        h.sent_iqs();
        let xml = "<query xmlns='jabber:iq:roster' ver='evil'>\
            <item jid='mallory@chord.localhost' subscription='both'/></query>";
        for from in [
            "bob@chord.localhost",
            "alice@chord.localhost/other",
            "chord.localhost",
        ] {
            let iq = push(Some(from), xml);
            assert!(!h.with_ctx(|ctx| on_iq(ctx, &iq)), "from {from}");
        }
        assert_eq!(contacts(&h).len(), 2);
        assert_eq!(
            stored_version(h.store.conn(), h.account_id).unwrap(),
            Some("v1".into())
        );
        assert!(h.take_sent().is_empty());
        assert!(h.take_dirty().is_empty());
    }

    #[test]
    fn push_with_two_items_or_junk_gets_an_error() {
        let mut h = Harness::new();
        let iq = push(
            None,
            "<query xmlns='jabber:iq:roster'>\
             <item jid='a@chord.localhost'/><item jid='b@chord.localhost'/></query>",
        );
        assert!(h.with_ctx(|ctx| on_iq(ctx, &iq)));
        assert!(matches!(h.sent_iqs()[0], Iq::Error { .. }));
        assert!(contacts(&h).is_empty());
    }

    #[test]
    fn other_iqs_are_not_roster_pushes() {
        let mut h = Harness::new();
        let iq = push(None, "<query xmlns='urn:example:other'/>");
        assert!(!h.with_ctx(|ctx| on_iq(ctx, &iq)));
        let get = Iq::from_get(
            "g",
            Roster {
                ver: None,
                items: vec![],
            },
        );
        assert!(!h.with_ctx(|ctx| on_iq(ctx, &get)));
    }

    #[test]
    fn presence_is_stored_and_cleared() {
        let mut h = Harness::new();
        let mut p = presence("bob@chord.localhost/phone", Type::None).with_show(Show::Dnd);
        p.set_status("", "Busy");
        p = p.with_priority(5);
        h.with_ctx(|ctx| on_presence(ctx, &p));
        let row: (Option<String>, Option<String>, i64) = h
            .store
            .conn()
            .query_row(
                "SELECT show, status, priority FROM presences WHERE jid = 'bob@chord.localhost/phone'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, (Some("dnd".into()), Some("Busy".into()), 5));
        let dirty = h.take_dirty();
        assert!(dirty.contains(&ViewKey::ChannelList(ChannelScope::Home)));
        assert!(dirty.contains(&ViewKey::MemberList(bare(BOB))));
        assert!(dirty.contains(&ViewKey::Timeline(bare(BOB))));

        // A second resource, then an update of the first.
        h.with_ctx(|ctx| on_presence(ctx, &presence("bob@chord.localhost/laptop", Type::None)));
        h.with_ctx(|ctx| on_presence(ctx, &presence("bob@chord.localhost/phone", Type::None)));
        let count = |h: &Harness| -> i64 {
            h.store
                .conn()
                .query_row("SELECT count(*) FROM presences", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count(&h), 2);
        h.with_ctx(|ctx| {
            on_presence(
                ctx,
                &presence("bob@chord.localhost/phone", Type::Unavailable),
            )
        });
        assert_eq!(count(&h), 1);
        h.with_ctx(|ctx| on_presence(ctx, &presence(BOB, Type::Unavailable)));
        assert_eq!(count(&h), 0);
    }

    #[test]
    fn presence_of_our_own_account_is_not_stored() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_presence(ctx, &presence("alice@chord.localhost/phone", Type::None)));
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT count(*) FROM presences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn subscription_request_is_reported_once() {
        let mut h = Harness::new();
        let p = presence(BOB, Type::Subscribe);
        h.with_ctx(|ctx| on_presence(ctx, &p));
        h.with_ctx(|ctx| on_presence(ctx, &p));
        let events: Vec<_> = h
            .effects
            .iter()
            .filter_map(|e| match e {
                crate::features::Effect::Emit(ClientEvent::SubscriptionRequest(j)) => Some(j),
                _ => None,
            })
            .collect();
        assert_eq!(events, vec![&bare(BOB)]);
    }

    #[test]
    fn approve_and_deny_send_presence() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Approve {
                    jid: bare(BOB),
                    reply,
                },
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Deny {
                    jid: bare(BOB),
                    reply,
                },
            )
        });
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let types: Vec<_> = h
            .take_sent()
            .into_iter()
            .map(|s| match s {
                Stanza::Presence(p) => (p.type_, p.to),
                other => panic!("{other:?}"),
            })
            .collect();
        let to = Some(Jid::new(BOB).unwrap());
        assert_eq!(
            types,
            vec![(Type::Subscribed, to.clone()), (Type::Unsubscribed, to)]
        );
    }

    #[test]
    fn unsubscribed_clears_the_presence() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_presence(ctx, &presence("bob@chord.localhost/phone", Type::None)));
        h.with_ctx(|ctx| on_presence(ctx, &presence(BOB, Type::Unsubscribed)));
        let n: i64 = h
            .store
            .conn()
            .query_row("SELECT count(*) FROM presences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn add_contact_sets_the_roster_then_subscribes() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Add {
                    jid: bare(BOB),
                    name: Some("Bob".into()),
                    reply,
                },
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("{iqs:?}")
        };
        let roster = Roster::try_from(payload.clone()).unwrap();
        assert_eq!(roster.items[0].jid, bare(BOB));
        assert_eq!(roster.items[0].name.as_deref(), Some("Bob"));
        // No answer and no subscribe before the server answers.
        assert_eq!(answer.try_recv().unwrap(), None);
        assert!(h.take_sent().is_empty());
        h.answer(is_add, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        assert!(matches!(&sent[0], Stanza::Presence(p) if p.type_ == Type::Subscribe));
    }

    #[test]
    fn add_contact_keeps_known_groups_and_name() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        h.sent_iqs();
        let (reply, _answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Add {
                    jid: bare(BOB),
                    name: None,
                    reply,
                },
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!()
        };
        let item = &Roster::try_from(payload.clone()).unwrap().items[0];
        assert_eq!(item.name.as_deref(), Some("Bob"));
        assert_eq!(item.groups.len(), 2);
    }

    #[test]
    fn add_contact_error_and_lost() {
        let mut h = Harness::new();
        for (response, expected) in [
            (
                IqResponse::Error(StanzaError::new(
                    ErrorType::Modify,
                    DefinedCondition::BadRequest,
                    "en",
                    "bad",
                )),
                ClientError::Server("BadRequest".into()),
            ),
            (IqResponse::Lost, ClientError::NotConnected),
        ] {
            let (reply, mut answer) = oneshot::channel();
            h.with_ctx(|ctx| {
                on_command(
                    ctx,
                    Command::Add {
                        jid: bare(BOB),
                        name: None,
                        reply,
                    },
                )
            });
            h.sent_iqs();
            h.respond(is_add, response);
            assert_eq!(answer.try_recv().unwrap(), Some(Err(expected)));
            // No subscribe request after a failure.
            assert!(h.take_sent().is_empty());
        }
    }

    #[test]
    fn remove_contact_answers_after_the_server() {
        let mut h = Harness::new();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Remove {
                    jid: bare(BOB),
                    reply,
                },
            )
        });
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!()
        };
        let roster = Roster::try_from(payload.clone()).unwrap();
        assert_eq!(roster.items[0].subscription, WireSubscription::Remove);
        assert_eq!(answer.try_recv().unwrap(), None);
        h.answer(is_remove, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));

        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Remove {
                    jid: bare(BOB),
                    reply,
                },
            )
        });
        h.respond(is_remove, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn contacts_command_reads_the_store_and_offline_fails() {
        let mut h = Harness::new();
        connect_with(&mut h, FULL);
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::Contacts { reply }));
        let list = answer.try_recv().unwrap().unwrap().unwrap();
        assert_eq!(list.len(), 2);

        let (reply, mut answer) = oneshot::channel();
        offline(Command::Contacts { reply });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
        let (reply, mut answer) = oneshot::channel();
        offline(Command::Add {
            jid: bare(BOB),
            name: None,
            reply,
        });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    fn is_preapprove(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Roster(Pending::Preapprove { .. }))
    }

    fn preapprove_cmd(h: &mut Harness, jid: &str) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        let jid = bare(jid);
        h.state.roster.pre_approval = true;
        h.with_ctx(|ctx| on_command(ctx, Command::Preapprove { jid, reply }));
        answer
    }

    fn approved_push(jid: &str, approved: bool) -> Iq {
        let attr = if approved { " approved='true'" } else { "" };
        push(
            Some("alice@chord.localhost"),
            &format!(
                "<query xmlns='jabber:iq:roster'><item jid='{jid}' subscription='none'{attr}/></query>"
            ),
        )
    }

    #[test]
    fn approved_flag_is_stored_and_ask_stays_separate() {
        let mut h = Harness::new();
        connect_with(
            &mut h,
            "<query xmlns='jabber:iq:roster' ver='v1'>\
             <item jid='bob@chord.localhost' subscription='none' approved='true'/>\
             <item jid='carol@chord.localhost' subscription='none' ask='subscribe' approved='true'/>\
             <item jid='dave@chord.localhost' subscription='none' ask='subscribe'/>\
             <item jid='erin@chord.localhost' subscription='none'/></query>",
        );
        let flags: Vec<_> = contacts(&h).iter().map(|c| (c.ask, c.approved)).collect();
        assert_eq!(
            flags,
            [(false, true), (true, true), (true, false), (false, false)]
        );
        // A push without the attribute clears the flag (RFC 6121, 3.4.2).
        let iq = approved_push(BOB, false);
        h.with_ctx(|ctx| assert!(on_iq(ctx, &iq)));
        assert!(!contacts(&h)[0].approved);
    }

    #[test]
    fn preapprove_needs_the_stream_feature() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_connected(ctx, &[]));
        assert!(!h.state.roster.pre_approval);
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                Command::Preapprove {
                    jid: bare(BOB),
                    reply,
                },
            )
        });
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Unsupported(_)))
        ));
        assert!(
            !h.take_sent()
                .iter()
                .any(|s| matches!(s, Stanza::Presence(_))),
            "no subscribed presence goes out"
        );
        h.with_ctx(|ctx| on_connected(ctx, &[NS_PRE_APPROVAL.to_owned()]));
        assert!(h.state.roster.pre_approval);
    }

    #[test]
    fn preapprove_sends_subscribed_then_a_ping_and_succeeds_on_the_push() {
        let mut h = Harness::new();
        let mut answer = preapprove_cmd(&mut h, BOB);
        let sent = h.take_sent();
        assert_eq!(sent.len(), 2);
        let Stanza::Presence(p) = &sent[0] else {
            panic!("{sent:?}")
        };
        assert_eq!(p.type_, Type::Subscribed);
        assert_eq!(p.to, Some(Jid::new(BOB).unwrap()));
        let Stanza::Iq(Iq::Get { payload, to, .. }) = &sent[1] else {
            panic!("{sent:?}")
        };
        assert!(payload.is("ping", ns::PING));
        assert!(to.is_none());
        assert_eq!(answer.try_recv().unwrap(), None);
        // The push comes before the answer to the ping.
        let iq = approved_push(BOB, true);
        h.with_ctx(|ctx| assert!(on_iq(ctx, &iq)));
        assert_eq!(answer.try_recv().unwrap(), None);
        h.answer(is_preapprove, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert!(contacts(&h)[0].approved);
        assert!(h.state.roster.preapprovals.is_empty());
    }

    #[test]
    fn preapprove_fails_when_no_push_comes_before_the_ping_answer() {
        let mut h = Harness::new();
        let mut answer = preapprove_cmd(&mut h, BOB);
        // A server without the feature answers the ping with an error or a result.
        h.respond(
            is_preapprove,
            IqResponse::Error(StanzaError::new(
                ErrorType::Cancel,
                DefinedCondition::ServiceUnavailable,
                "en",
                "no",
            )),
        );
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Unsupported(_)))
        ));
    }

    #[test]
    fn preapprove_reports_a_lost_session() {
        let mut h = Harness::new();
        let mut answer = preapprove_cmd(&mut h, BOB);
        h.respond(is_preapprove, IqResponse::Lost);
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn two_preapprovals_of_one_contact_send_one_presence() {
        let mut h = Harness::new();
        let mut first = preapprove_cmd(&mut h, BOB);
        let mut second = preapprove_cmd(&mut h, BOB);
        assert_eq!(h.take_sent().len(), 2);
        let iq = approved_push(BOB, true);
        h.with_ctx(|ctx| assert!(on_iq(ctx, &iq)));
        h.answer(is_preapprove, None);
        assert_eq!(first.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(second.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn preapprove_needs_nothing_for_an_approved_or_subscribed_contact() {
        let mut h = Harness::new();
        connect_with(
            &mut h,
            "<query xmlns='jabber:iq:roster' ver='v1'>\
             <item jid='bob@chord.localhost' subscription='none' approved='true'/>\
             <item jid='carol@chord.localhost' subscription='from'/></query>",
        );
        h.take_sent();
        for jid in [BOB, "carol@chord.localhost"] {
            let mut answer = preapprove_cmd(&mut h, jid);
            assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        }
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn preapprove_after_a_request_is_a_normal_approval() {
        let mut h = Harness::new();
        h.with_ctx(|ctx| on_presence(ctx, &presence(BOB, Type::Subscribe)));
        h.take_sent();
        let mut answer = preapprove_cmd(&mut h, BOB);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        let sent = h.take_sent();
        assert_eq!(sent.len(), 1);
        assert!(matches!(&sent[0], Stanza::Presence(p) if p.type_ == Type::Subscribed));
        assert!(h.state.roster.requests.is_empty());
    }

    #[test]
    fn preapprove_rejects_our_own_account_and_fails_offline() {
        let mut h = Harness::new();
        let mut answer = preapprove_cmd(&mut h, "alice@chord.localhost");
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Invalid(_)))
        ));
        assert!(h.take_sent().is_empty());
        let (reply, mut answer) = oneshot::channel();
        offline(Command::Preapprove {
            jid: bare(BOB),
            reply,
        });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }

    #[test]
    fn groups_json_round_trips() {
        let groups = ["Friends", "a \"quoted\" \\ one", "line\nbreak", "ünï \u{1}"];
        let json = groups_to_json(&groups);
        assert_eq!(groups_from_json(&json), groups);
        assert_eq!(groups_to_json(&[]), "[]");
        assert!(groups_from_json("[]").is_empty());
        assert!(groups_from_json("not json").is_empty());
    }
}
