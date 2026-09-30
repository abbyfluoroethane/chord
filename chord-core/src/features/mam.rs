//! Message archive management (XEP-0313): catch-up after a new session, and older history for a timeline.
//!
//! - The account archive holds the 1:1 chats. The archive of a room holds its messages.
//! - `sync_cursors` keeps, for each archive, the newest and the oldest archive id that
//!   we stored, and whether the oldest page arrived. The row for '' is the account
//!   archive. The row for a room JID is that room. The row for a chat peer only tracks
//!   the completeness of the history with that peer.
//! - The account archive needs `urn:xmpp:mam:2` in the disco of the server or the account.
//!   The first query waits for the end of service discovery, and a server without MAM gets
//!   no query.
//! - A query that fails with a temporary error, or with an answer that does not parse, runs
//!   again with a growing delay (`RETRY_TICKS`), at most three times. A permanent error
//!   does not run again.
//! - When the catch-up of the account archive is done, a live message that carries a
//!   stanza-id of our account moves `newest_id` (`on_live`). A reconnect then asks only for
//!   what came after it.
//! - A query has a random `queryid`. We accept a result only for a running query, and
//!   only from the entity that we asked (XEP-0313, section 8: anyone can forge a result).

use futures_channel::oneshot;
use jid::{BareJid, Jid};
use rusqlite::{OptionalExtension, params};
use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::mam::{Fin, Query, QueryId, Result_};
use xmpp_parsers::message::Message;
use xmpp_parsers::ns;
use xmpp_parsers::rsm::SetQuery;
use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType, StanzaError};

use super::{Ctx, IqResponse, Pending as FeaturePending, chat, muc, new_id};
use crate::actor::{ClientError, ClientHandle};
use crate::views::ViewKey;

/// The number of messages that we ask for in one page.
const PAGE_SIZE: usize = 50;

/// The wait before each retry of a failed query, in session ticks (15 s each): 15 s, 1 min,
/// 4 min. After the last one, the query stays failed until the next connect.
const RETRY_TICKS: [u32; 3] = [1, 4, 16];

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// The queries that wait for their final answer, by queryid.
    queries: std::collections::HashMap<String, Running>,
    /// The failed queries that wait to run again.
    retries: Vec<Retry>,
    /// True when the account catch-up of this session has ended. Only then does a live
    /// message move the cursor, so that no gap stays behind it.
    account_synced: bool,
}

/// A failed query that waits to run again.
#[derive(Debug)]
struct Retry {
    target: Target,
    kind: Kind,
    after: Option<String>,
    /// How many retries ran already.
    attempt: usize,
    ticks_left: u32,
}

/// One query that waits for its answer.
#[derive(Debug)]
struct Running {
    target: Target,
    kind: Kind,
    /// The id that the query asks after (a catch-up) or before.
    after: Option<String>,
    /// How many retries ran before this query.
    attempt: usize,
}

/// The archive that a query asks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// The account archive, without a filter.
    Account,
    /// The account archive, with the `with` filter for one peer.
    Chat(BareJid),
    /// The archive of a room.
    Room(BareJid),
}

impl Target {
    /// The key in `sync_cursors`.
    fn archive(&self) -> String {
        match self {
            Self::Account => String::new(),
            Self::Chat(jid) | Self::Room(jid) => jid.to_string(),
        }
    }

    /// The entity that gets the query and sends the results. `None` is our own account.
    fn entity(&self) -> Option<Jid> {
        match self {
            Self::Room(room) => Some(Jid::from(room.clone())),
            _ => None,
        }
    }

    /// The timeline that shows the messages of this archive.
    fn timeline(&self) -> Option<ViewKey> {
        match self {
            Self::Account => None,
            Self::Chat(jid) | Self::Room(jid) => Some(ViewKey::Timeline(jid.clone())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Forward from the newest stored id, page by page.
    CatchUp,
    /// One page before an id. An empty id asks for the newest page.
    Backward,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The final answer of the query with this queryid.
    Page(String),
}

/// A command from the public API.
pub(crate) enum Command {
    CatchUp {
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
    Older {
        peer: BareJid,
        reply: oneshot::Sender<Result<(), ClientError>>,
    },
}

impl ClientHandle {
    /// Fetch what the account archive got since the last sync. It returns when the query
    /// is queued. The messages arrive as `MessageReceived` events and view diffs.
    pub async fn sync_archive(&self) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Mam(Command::CatchUp { reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }

    /// Fetch one page of history before the oldest stored message of a chat or a room.
    /// It returns when the query is queued. It does nothing when a query for `peer`
    /// runs or the history is complete.
    pub async fn load_older(&self, peer: BareJid) -> Result<(), ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(super::FeatureCommand::Mam(Command::Older { peer, reply }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

/// The sync state of one archive.
#[derive(Debug, Default, PartialEq, Eq)]
struct Cursor {
    newest_id: Option<String>,
    oldest_id: Option<String>,
    history_complete: bool,
}

fn load_cursor(ctx: &Ctx<'_>, archive: &str) -> Cursor {
    let row = ctx
        .store
        .conn()
        .query_row(
            "SELECT newest_id, oldest_id, history_complete FROM sync_cursors
             WHERE account_id = ?1 AND archive = ?2",
            params![ctx.account_id, archive],
            |row| {
                Ok(Cursor {
                    newest_id: row.get(0)?,
                    oldest_id: row.get(1)?,
                    history_complete: row.get::<_, i64>(2)? != 0,
                })
            },
        )
        .optional();
    match row {
        Ok(cursor) => cursor.unwrap_or_default(),
        Err(e) => {
            ctx.store_error("read a sync cursor", e);
            Cursor::default()
        }
    }
}

fn save_cursor(ctx: &Ctx<'_>, archive: &str, cursor: &Cursor) {
    let result = ctx.store.conn().execute(
        "INSERT INTO sync_cursors (account_id, archive, newest_id, oldest_id, history_complete)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (account_id, archive) DO UPDATE SET
             newest_id = excluded.newest_id,
             oldest_id = excluded.oldest_id,
             history_complete = excluded.history_complete",
        params![
            ctx.account_id,
            archive,
            cursor.newest_id,
            cursor.oldest_id,
            cursor.history_complete
        ],
    );
    if let Err(e) = result {
        ctx.store_error("write a sync cursor", e);
    }
}

/// The oldest archive id among the stored messages with `peer`.
fn oldest_stored_id(ctx: &Ctx<'_>, peer: &str) -> Option<String> {
    let row = ctx
        .store
        .conn()
        .query_row(
            "SELECT key FROM messages
             WHERE account_id = ?1 AND peer = ?2 AND key_kind = 'stanza-id'
             ORDER BY timestamp, id LIMIT 1",
            params![ctx.account_id, peer],
            |row| row.get(0),
        )
        .optional();
    match row {
        Ok(id) => id,
        Err(e) => {
            ctx.store_error("read the oldest archive id", e);
            None
        }
    }
}

fn is_room(ctx: &Ctx<'_>, jid: &BareJid) -> bool {
    let row = ctx
        .store
        .conn()
        .query_row(
            "SELECT 1 FROM rooms WHERE account_id = ?1 AND jid = ?2",
            params![ctx.account_id, jid.to_string()],
            |_| Ok(()),
        )
        .optional();
    match row {
        Ok(found) => found.is_some(),
        Err(e) => {
            ctx.store_error("read the rooms", e);
            false
        }
    }
}

fn is_running(ctx: &Ctx<'_>, target: &Target) -> bool {
    ctx.state.mam.queries.values().any(|q| q.target == *target)
}

/// Whether the server can have the archive of `target`. A room archive cannot be checked
/// here: the room answers. The account archive needs `urn:xmpp:mam:2` in the disco of the
/// server or the account, and is unknown until the discovery ends.
fn server_supports(ctx: &Ctx<'_>, target: &Target) -> bool {
    match target {
        Target::Room(_) => true,
        _ => ctx.state.disco.complete && ctx.state.disco.server_has(ns::MAM),
    }
}

/// Send one query and remember it.
fn send_query(ctx: &mut Ctx<'_>, target: Target, kind: Kind, after: Option<String>) {
    send_attempt(ctx, target, kind, after, 0);
}

/// Send one query. `attempt` counts the retries that ran before it.
fn send_attempt(
    ctx: &mut Ctx<'_>,
    target: Target,
    kind: Kind,
    after: Option<String>,
    attempt: usize,
) {
    if !server_supports(ctx, &target) {
        log::debug!(
            "no MAM query for {:?}: the server has no urn:xmpp:mam:2",
            target.archive()
        );
        return;
    }
    // A new query replaces a retry that waits for the same archive.
    ctx.state.mam.retries.retain(|r| r.target != target);
    let queryid = new_id();
    let after_copy = after.clone();
    let mut set = SetQuery {
        max: Some(PAGE_SIZE),
        after: None,
        before: None,
        index: None,
    };
    match (kind, after) {
        (Kind::CatchUp, Some(id)) => set.after = Some(id),
        // The empty `before` asks for the last page (XEP-0059, section 2.5).
        (_, Some(id)) => set.before = Some(id),
        (_, None) => set.before = Some(String::new()),
    }
    let form = match &target {
        Target::Chat(peer) => Some(DataForm::new(
            DataFormType::Submit,
            ns::MAM,
            vec![Field::new("with", FieldType::JidSingle).with_value(&peer.to_string())],
        )),
        _ => None,
    };
    let query = Query {
        queryid: Some(QueryId(queryid.clone())),
        node: None,
        form,
        set: Some(set),
        flip_page: false,
    };
    let mut iq = Iq::from_set("", query);
    if let Some(entity) = target.entity() {
        iq = iq.with_to(entity);
    }
    ctx.state.mam.queries.insert(
        queryid.clone(),
        Running {
            target,
            kind,
            after: after_copy,
            attempt,
        },
    );
    ctx.request(iq, FeaturePending::Mam(Pending::Page(queryid)));
}

/// True if the error can go away, so that the same query can work later.
fn is_temporary(error: &StanzaError) -> bool {
    error.type_ == ErrorType::Wait
        || matches!(
            error.defined_condition,
            DefinedCondition::InternalServerError
                | DefinedCondition::RemoteServerTimeout
                | DefinedCondition::ResourceConstraint
                | DefinedCondition::UndefinedCondition
        )
}

/// Fetch what an archive got after the newest stored id. With no cursor, fetch only the
/// newest page.
fn catch_up(ctx: &mut Ctx<'_>, target: Target) {
    if is_running(ctx, &target) {
        return;
    }
    // The discovery decides if there is an archive. `on_services_ready` tries again.
    if target == Target::Account && !ctx.state.disco.complete {
        return;
    }
    let cursor = load_cursor(ctx, &target.archive());
    match cursor.newest_id {
        Some(id) => send_query(ctx, target, Kind::CatchUp, Some(id)),
        None => send_query(ctx, target, Kind::Backward, None),
    }
}

pub(crate) fn on_connected(ctx: &mut Ctx<'_>) {
    catch_up(ctx, Target::Account);
}

/// Service discovery ended: the account archive can start (or stay off).
pub(crate) fn on_services_ready(ctx: &mut Ctx<'_>) {
    catch_up(ctx, Target::Account);
}

/// Run the query again after a failure, with a delay, or give up after the last delay.
fn schedule_retry(ctx: &mut Ctx<'_>, run: Running) {
    let Some(&ticks) = RETRY_TICKS.get(run.attempt) else {
        log::warn!("giving up the MAM query for {:?}", run.target.archive());
        return;
    };
    ctx.state.mam.retries.push(Retry {
        target: run.target,
        kind: run.kind,
        after: run.after,
        attempt: run.attempt + 1,
        ticks_left: ticks,
    });
}

/// A session tick: run the retries that are due.
pub(crate) fn on_tick(ctx: &mut Ctx<'_>) {
    if ctx.state.mam.retries.is_empty() {
        return;
    }
    let mut due = Vec::new();
    ctx.state.mam.retries.retain_mut(|r| {
        r.ticks_left = r.ticks_left.saturating_sub(1);
        if r.ticks_left == 0 {
            due.push(Retry {
                target: r.target.clone(),
                kind: r.kind,
                after: r.after.clone(),
                attempt: r.attempt,
                ticks_left: 0,
            });
            false
        } else {
            true
        }
    });
    for retry in due {
        if !is_running(ctx, &retry.target) {
            send_attempt(ctx, retry.target, retry.kind, retry.after, retry.attempt);
        }
    }
}

/// A live chat message carries the stanza-id `id` of our account. When the catch-up of
/// this session has ended, `id` is the newest archive position that we know, so the next
/// reconnect need not fetch what we have seen live.
pub(crate) fn on_live(ctx: &mut Ctx<'_>, id: &str) {
    if !ctx.state.mam.account_synced {
        return;
    }
    let mut cursor = load_cursor(ctx, "");
    if cursor.newest_id.as_deref() == Some(id) {
        return;
    }
    cursor.newest_id = Some(id.to_owned());
    save_cursor(ctx, "", &cursor);
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    let Pending::Page(queryid) = pending;
    // Forget the query first. Whatever the answer is, a later request can start again.
    let Some(run) = ctx.state.mam.queries.remove(&queryid) else {
        return;
    };
    let archive = run.target.archive();
    let fin = match response {
        IqResponse::Result(Some(payload)) => match Fin::try_from(payload) {
            Ok(fin) => fin,
            Err(e) => {
                log::warn!("invalid MAM answer for {archive:?}: {e:?}");
                schedule_retry(ctx, run);
                return;
            }
        },
        IqResponse::Result(None) => {
            log::warn!("empty MAM answer for {archive:?}");
            schedule_retry(ctx, run);
            return;
        }
        IqResponse::Error(e) => {
            log::warn!("MAM query for {archive:?} failed: {e:?}");
            // The server does not know our newest id, for example after a purge.
            // Start again with the newest page.
            if run.kind == Kind::CatchUp && e.defined_condition == DefinedCondition::ItemNotFound {
                save_cursor(ctx, &archive, &Cursor::default());
                send_query(ctx, run.target, Kind::Backward, None);
            } else if is_temporary(&e) {
                schedule_retry(ctx, run);
            }
            return;
        }
        IqResponse::Lost => return,
    };

    let mut cursor = load_cursor(ctx, &archive);
    let first = fin.set.first.as_ref().map(|f| f.item.clone());
    let last = fin.set.last.clone();
    let mut next = None;
    match run.kind {
        Kind::CatchUp => {
            if let Some(last) = last
                && cursor.newest_id.as_ref() != Some(&last)
            {
                if !fin.complete {
                    next = Some(last.clone());
                }
                cursor.newest_id = Some(last);
            }
            if cursor.oldest_id.is_none() {
                cursor.oldest_id = first;
            }
        }
        Kind::Backward => {
            if cursor.newest_id.is_none() {
                cursor.newest_id = last;
            }
            if first.is_some() {
                cursor.oldest_id = first.clone();
            }
            // Some servers set `complete` for any page that reaches the end of the
            // archive. The index of the first item tells the truth when it is there.
            let at_start = fin
                .set
                .first
                .as_ref()
                .is_none_or(|f| f.index.is_none_or(|i| i == 0));
            if fin.complete && at_start {
                cursor.history_complete = true;
            }
        }
    }
    save_cursor(ctx, &archive, &cursor);
    if run.target == Target::Account && next.is_none() {
        ctx.state.mam.account_synced = true;
    }
    if let Some(view) = run.target.timeline() {
        ctx.changed(view);
    }
    if let Some(after) = next {
        send_query(ctx, run.target, Kind::CatchUp, Some(after));
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::CatchUp { reply } => {
            catch_up(ctx, Target::Account);
            let _ = reply.send(Ok(()));
        }
        Command::Older { peer, reply } => {
            need_older(ctx, &peer);
            let _ = reply.send(Ok(()));
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::CatchUp { reply } | Command::Older { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

/// A message that carries a MAM result. Returns true if it is one.
pub(crate) fn on_result(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    let Some(payload) = message.payloads.iter().find(|p| p.is("result", ns::MAM)) else {
        return false;
    };
    let Ok(result) = Result_::try_from(payload.clone()) else {
        log::debug!("dropped a MAM result that does not parse");
        return true;
    };
    let Some(run) = result
        .queryid
        .as_ref()
        .and_then(|id| ctx.state.mam.queries.get(&id.0))
    else {
        log::warn!("dropped a MAM result for an unknown query");
        return true;
    };
    let from_ok = match (&run.target, &message.from) {
        (Target::Room(room), Some(from)) => from.is_bare() && from.to_bare() == *room,
        (Target::Room(_), None) => false,
        (_, None) => true,
        (_, Some(from)) => from.is_bare() && from.to_bare() == *ctx.account,
    };
    if !from_ok {
        log::warn!("dropped a MAM result from {:?}", message.from);
        return true;
    }
    let target = run.target.clone();
    let timestamp = result
        .forwarded
        .delay
        .as_ref()
        .map(|d| d.stamp.0.timestamp_millis());
    let inner = result.forwarded.message;
    match &target {
        Target::Room(room) => muc::store_archived(ctx, room, &inner, &result.id, timestamp),
        _ => chat::store_archived(ctx, &inner, &result.id, timestamp),
    }
    true
}

/// A timeline wants messages older than the oldest stored one.
pub(crate) fn need_older(ctx: &mut Ctx<'_>, room: &BareJid) {
    let target = if is_room(ctx, room) {
        Target::Room(room.clone())
    } else {
        Target::Chat(room.clone())
    };
    if is_running(ctx, &target) {
        return;
    }
    let archive = target.archive();
    let cursor = load_cursor(ctx, &archive);
    // A complete account archive holds the whole history of every chat.
    let account_complete = matches!(target, Target::Chat(_))
        && load_cursor(ctx, &Target::Account.archive()).history_complete;
    if cursor.history_complete || account_complete {
        return;
    }
    let anchor = cursor
        .oldest_id
        .or_else(|| oldest_stored_id(ctx, &room.to_string()));
    send_query(ctx, target, Kind::Backward, anchor);
}

/// Fetch the room archive after a join: the messages after the newest stored one.
/// The MUC module calls it when the join completes.
pub(crate) fn catch_up_room(ctx: &mut Ctx<'_>, room: &BareJid) {
    catch_up(ctx, Target::Room(room.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::testing::Harness;
    use crate::store::queries::messages_with;
    use xmpp_parsers::date::DateTime;
    use xmpp_parsers::delay::Delay;
    use xmpp_parsers::forwarding::Forwarded;
    use xmpp_parsers::minidom::Element;
    use xmpp_parsers::rsm::{First, SetResult};
    use xmpp_parsers::stanza_error::{ErrorType, StanzaError};

    /// A harness whose server discovery is done, and has MAM.
    fn ready() -> Harness {
        let mut h = Harness::new();
        let mut info = crate::features::disco::info(None);
        info.features.insert(ns::MAM.into());
        h.state.disco.server = Some(info);
        h.state.disco.complete = true;
        h
    }

    fn is_mam(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Mam(_))
    }

    fn room() -> BareJid {
        BareJid::new("room@rooms.chord.localhost").unwrap()
    }

    fn bob() -> BareJid {
        BareJid::new("bob@chord.localhost").unwrap()
    }

    /// The single MAM query that the feature sent, with its queryid.
    fn sent_query(h: &mut Harness) -> (Iq, Query, String) {
        let mut iqs = h.sent_iqs();
        assert_eq!(iqs.len(), 1, "{iqs:?}");
        let iq = iqs.remove(0);
        let Iq::Set { payload, .. } = &iq else {
            panic!("{iq:?}")
        };
        let query = Query::try_from(payload.clone()).unwrap();
        let queryid = query.queryid.clone().unwrap().0;
        (iq, query, queryid)
    }

    fn fin(complete: bool, first: Option<(&str, Option<usize>)>, last: Option<&str>) -> Element {
        Fin {
            complete,
            set: SetResult {
                first: first.map(|(item, index)| First {
                    index,
                    item: item.to_owned(),
                }),
                last: last.map(str::to_owned),
                count: None,
            },
        }
        .into()
    }

    /// A result message that wraps a message from bob to us.
    fn result(queryid: &str, id: &str, from: Option<&str>, body: &str, stamp: &str) -> Message {
        let mut inner = Message::chat(Jid::new("alice@chord.localhost").unwrap())
            .with_body("".into(), body.into());
        inner.from = Some(Jid::new("bob@chord.localhost/phone").unwrap());
        let forwarded = Forwarded {
            delay: Some(Delay {
                from: None,
                stamp: stamp.parse::<DateTime>().unwrap(),
                data: None,
            }),
            message: inner,
        };
        let mut message = Message::new(None).with_payload(Result_ {
            id: id.into(),
            queryid: Some(QueryId(queryid.into())),
            forwarded,
        });
        message.from = from.map(|f| Jid::new(f).unwrap());
        message
    }

    fn feed(h: &mut Harness, message: &Message) -> bool {
        h.with_ctx(|ctx| on_result(ctx, message))
    }

    fn bodies(h: &Harness) -> Vec<String> {
        messages_with(h.store.conn(), h.account_id, "bob@chord.localhost")
            .unwrap()
            .into_iter()
            .map(|m| m.body)
            .collect()
    }

    fn cursor(h: &mut Harness, archive: &str) -> Cursor {
        h.with_ctx(|ctx| load_cursor(ctx, archive))
    }

    fn set_cursor(h: &mut Harness, archive: &str, c: Cursor) {
        h.with_ctx(|ctx| save_cursor(ctx, archive, &c));
    }

    fn add_room(h: &Harness) {
        h.store
            .conn()
            .execute(
                "INSERT INTO rooms (account_id, jid) VALUES (?1, ?2)",
                params![h.account_id, room().to_string()],
            )
            .unwrap();
    }

    fn error(condition: DefinedCondition) -> IqResponse {
        IqResponse::Error(StanzaError::new(ErrorType::Cancel, condition, "en", "test"))
    }

    #[test]
    fn first_login_fetches_only_the_newest_page() {
        let mut h = ready();
        h.with_ctx(on_connected);
        let (iq, query, qid) = sent_query(&mut h);
        assert_eq!(iq.to(), None);
        let set = query.set.clone().unwrap();
        assert_eq!(set.max, Some(50));
        assert_eq!(set.before, Some(String::new()));
        assert_eq!(set.after, None);
        assert!(query.form.is_none());
        // The empty `before` must be on the wire.
        let xml = String::from(&Element::from(query));
        assert!(xml.contains("<before"), "{xml}");

        let stamp = "2026-01-01T10:00:00Z";
        assert!(feed(&mut h, &result(&qid, "a1", None, "one", stamp)));
        assert!(feed(&mut h, &result(&qid, "a2", None, "two", stamp)));
        h.answer(is_mam, Some(fin(false, Some(("a1", Some(90))), Some("a2"))));
        // No further page, even though the server has older messages.
        assert!(h.sent_iqs().is_empty());
        assert_eq!(bodies(&h), ["one", "two"]);
        assert_eq!(
            cursor(&mut h, ""),
            Cursor {
                newest_id: Some("a2".into()),
                oldest_id: Some("a1".into()),
                history_complete: false,
            }
        );
    }

    #[test]
    fn a_short_archive_is_complete() {
        let mut h = ready();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.answer(is_mam, Some(fin(true, Some(("a1", Some(0))), Some("a2"))));
        assert!(cursor(&mut h, "").history_complete);
    }

    #[test]
    fn complete_with_a_later_first_index_is_not_complete() {
        let mut h = ready();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.answer(is_mam, Some(fin(true, Some(("a1", Some(3))), Some("a2"))));
        assert!(!cursor(&mut h, "").history_complete);
    }

    #[test]
    fn catch_up_pages_forward_with_after_until_complete() {
        let mut h = ready();
        let start = Cursor {
            newest_id: Some("a5".into()),
            oldest_id: Some("a1".into()),
            history_complete: false,
        };
        set_cursor(&mut h, "", start);
        h.with_ctx(on_connected);
        let (_, query, qid) = sent_query(&mut h);
        let set = query.set.unwrap();
        assert_eq!(set.after.as_deref(), Some("a5"));
        assert_eq!(set.before, None);

        let stamp = "2026-01-02T10:00:00Z";
        assert!(feed(&mut h, &result(&qid, "a6", None, "six", stamp)));
        h.answer(is_mam, Some(fin(false, Some(("a6", None)), Some("a6"))));
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a6"));
        assert_eq!(cursor(&mut h, "").oldest_id.as_deref(), Some("a1"));

        let (_, query, qid2) = sent_query(&mut h);
        assert_ne!(qid, qid2);
        assert_eq!(query.set.unwrap().after.as_deref(), Some("a6"));
        // The first query is over. A late result for it is dropped.
        assert!(feed(&mut h, &result(&qid, "a7", None, "late", stamp)));
        assert!(feed(&mut h, &result(&qid2, "a7", None, "seven", stamp)));
        h.answer(is_mam, Some(fin(true, Some(("a7", None)), Some("a7"))));
        assert!(h.sent_iqs().is_empty());
        assert_eq!(bodies(&h), ["six", "seven"]);
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a7"));
        assert!(h.state.mam.queries.is_empty());
    }

    #[test]
    fn catch_up_stops_when_the_cursor_does_not_move() {
        let mut h = ready();
        let start = Cursor {
            newest_id: Some("a5".into()),
            ..Cursor::default()
        };
        set_cursor(&mut h, "", start);
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.answer(is_mam, Some(fin(false, Some(("a5", None)), Some("a5"))));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn unknown_cursor_falls_back_to_the_newest_page() {
        let mut h = ready();
        let start = Cursor {
            newest_id: Some("gone".into()),
            oldest_id: Some("older".into()),
            history_complete: false,
        };
        set_cursor(&mut h, "", start);
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.respond(is_mam, error(DefinedCondition::ItemNotFound));
        assert_eq!(cursor(&mut h, ""), Cursor::default());
        let (_, query, _) = sent_query(&mut h);
        assert_eq!(query.set.unwrap().before, Some(String::new()));
    }

    #[test]
    fn results_with_a_foreign_queryid_or_sender_are_dropped() {
        let mut h = ready();
        h.with_ctx(on_connected);
        let (_, _, qid) = sent_query(&mut h);
        let stamp = "2026-01-01T10:00:00Z";
        // Consumed, not stored.
        assert!(feed(
            &mut h,
            &result("other", "x1", None, "foreign id", stamp)
        ));
        for (id, from) in [
            ("x2", "mallory@evil.example"),
            ("x3", "alice@chord.localhost/other"),
            ("x4", "room@rooms.chord.localhost"),
        ] {
            assert!(feed(
                &mut h,
                &result(&qid, id, Some(from), "foreign from", stamp)
            ));
        }
        assert!(bodies(&h).is_empty());
        // Our own bare JID and no `from` pass.
        let own = Some("alice@chord.localhost");
        assert!(feed(&mut h, &result(&qid, "x5", own, "own", stamp)));
        assert!(feed(&mut h, &result(&qid, "x6", None, "none", stamp)));
        assert_eq!(bodies(&h), ["own", "none"]);
        // A message without a MAM result is not ours.
        let plain = Message::chat(None).with_body("".into(), "hi".into());
        assert!(!feed(&mut h, &plain));
    }

    #[test]
    fn archive_timestamp_comes_from_the_delay() {
        let mut h = ready();
        h.with_ctx(on_connected);
        let (_, _, qid) = sent_query(&mut h);
        feed(
            &mut h,
            &result(&qid, "a1", None, "one", "2026-01-01T10:00:00Z"),
        );
        let messages = messages_with(h.store.conn(), h.account_id, "bob@chord.localhost").unwrap();
        assert_eq!(messages[0].timestamp, 1_767_261_600_000);
        assert_eq!(messages[0].key, "a1");
    }

    #[test]
    fn need_older_for_a_chat_asks_the_account_archive_with_a_filter() {
        let mut h = ready();
        // One stored archive message gives the anchor.
        h.with_ctx(on_connected);
        let (_, _, qid) = sent_query(&mut h);
        feed(
            &mut h,
            &result(&qid, "a7", None, "seven", "2026-01-01T10:00:00Z"),
        );
        h.answer(is_mam, Some(fin(false, Some(("a7", Some(5))), Some("a7"))));

        h.with_ctx(|ctx| need_older(ctx, &bob()));
        let (iq, query, qid) = sent_query(&mut h);
        assert_eq!(iq.to(), None);
        let set = query.set.clone().unwrap();
        assert_eq!(set.before.as_deref(), Some("a7"));
        assert_eq!(set.max, Some(50));
        let form = query.form.unwrap();
        assert_eq!(form.type_, DataFormType::Submit);
        assert_eq!(form.form_type(), Some(ns::MAM));
        let with = form
            .fields
            .iter()
            .find(|f| f.var.as_deref() == Some("with"));
        assert_eq!(with.unwrap().values, ["bob@chord.localhost"]);

        // A second call while the query runs does nothing.
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert!(h.sent_iqs().is_empty());

        feed(
            &mut h,
            &result(&qid, "a3", None, "three", "2025-12-31T10:00:00Z"),
        );
        h.take_dirty();
        h.answer(is_mam, Some(fin(false, Some(("a3", Some(1))), Some("a6"))));
        assert!(h.take_dirty().contains(&ViewKey::Timeline(bob())));
        let peer = cursor(&mut h, "bob@chord.localhost");
        assert_eq!(peer.oldest_id.as_deref(), Some("a3"));
        // The account cursor is not touched.
        assert_eq!(cursor(&mut h, "").oldest_id.as_deref(), Some("a7"));

        // The next page starts before the new oldest id.
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        let (_, query, _) = sent_query(&mut h);
        assert_eq!(query.set.unwrap().before.as_deref(), Some("a3"));

        // The end of the history sets the flag. Then need_older does nothing.
        h.answer(is_mam, Some(fin(true, Some(("a1", Some(0))), Some("a2"))));
        assert!(cursor(&mut h, "bob@chord.localhost").history_complete);
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn need_older_for_a_chat_stops_when_the_account_archive_is_complete() {
        let mut h = ready();
        let done = Cursor {
            history_complete: true,
            ..Cursor::default()
        };
        set_cursor(&mut h, "", done);
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn need_older_for_a_room_asks_the_room_archive() {
        let mut h = ready();
        add_room(&h);
        let start = Cursor {
            newest_id: Some("r9".into()),
            oldest_id: Some("r5".into()),
            history_complete: false,
        };
        set_cursor(&mut h, &room().to_string(), start);
        h.with_ctx(|ctx| need_older(ctx, &room()));
        let (iq, query, qid) = sent_query(&mut h);
        assert_eq!(iq.to(), Some(&Jid::from(room())));
        assert_eq!(query.set.unwrap().before.as_deref(), Some("r5"));
        assert!(query.form.is_none());

        // A room result must come from the room.
        let stamp = "2026-01-01T10:00:00Z";
        assert!(feed(&mut h, &result(&qid, "r4", None, "no from", stamp)));
        let own = Some("alice@chord.localhost");
        assert!(feed(&mut h, &result(&qid, "r4", own, "own from", stamp)));
        assert!(bodies(&h).is_empty());

        h.answer(is_mam, Some(fin(true, Some(("r1", Some(0))), Some("r4"))));
        let c = cursor(&mut h, &room().to_string());
        assert_eq!(c.oldest_id.as_deref(), Some("r1"));
        assert_eq!(c.newest_id.as_deref(), Some("r9"));
        assert!(c.history_complete);
        assert!(h.take_dirty().contains(&ViewKey::Timeline(room())));
        h.with_ctx(|ctx| need_older(ctx, &room()));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn room_catch_up_uses_after_and_the_room_cursor() {
        let mut h = ready();
        add_room(&h);
        // No cursor: the newest page.
        h.with_ctx(|ctx| catch_up_room(ctx, &room()));
        let (iq, query, _) = sent_query(&mut h);
        assert_eq!(iq.to(), Some(&Jid::from(room())));
        assert_eq!(query.set.unwrap().before, Some(String::new()));
        h.answer(is_mam, Some(fin(false, Some(("r5", Some(4))), Some("r9"))));

        h.with_ctx(|ctx| catch_up_room(ctx, &room()));
        let (_, query, _) = sent_query(&mut h);
        assert_eq!(query.set.unwrap().after.as_deref(), Some("r9"));
        h.answer(is_mam, Some(fin(false, Some(("r10", None)), Some("r12"))));
        let (_, query, _) = sent_query(&mut h);
        assert_eq!(query.set.unwrap().after.as_deref(), Some("r12"));
        h.answer(is_mam, Some(fin(true, None, None)));
        assert!(h.sent_iqs().is_empty());
        let c = cursor(&mut h, &room().to_string());
        assert_eq!(c.newest_id.as_deref(), Some("r12"));
    }

    #[test]
    fn error_and_lost_answers_reset_the_query_state() {
        let mut h = ready();
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert_eq!(h.sent_iqs().len(), 1);
        h.respond(is_mam, error(DefinedCondition::ServiceUnavailable));
        assert!(h.state.mam.queries.is_empty());
        assert!(!cursor(&mut h, "bob@chord.localhost").history_complete);

        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert_eq!(h.sent_iqs().len(), 1);
        h.respond(is_mam, IqResponse::Lost);
        assert!(h.state.mam.queries.is_empty());

        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert_eq!(h.sent_iqs().len(), 1);
        h.answer(is_mam, None);
        assert!(h.state.mam.queries.is_empty());
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert_eq!(h.sent_iqs().len(), 1);
    }

    #[test]
    fn a_server_without_mam_gets_no_query() {
        let mut h = Harness::new();
        // Discovery is not done: the account catch-up waits.
        h.with_ctx(on_connected);
        assert!(h.sent_iqs().is_empty());
        // Discovery is done and the server has no MAM: still no query.
        h.state.disco.server = Some(crate::features::disco::info(None));
        h.state.disco.complete = true;
        h.with_ctx(on_services_ready);
        h.with_ctx(|ctx| need_older(ctx, &bob()));
        assert!(h.sent_iqs().is_empty());
        // With MAM in the disco, the services-ready hook starts the catch-up.
        let mut h = ready();
        h.state.disco.complete = false;
        h.with_ctx(on_connected);
        assert!(h.sent_iqs().is_empty());
        h.state.disco.complete = true;
        h.with_ctx(on_services_ready);
        assert_eq!(h.sent_iqs().len(), 1);
    }

    fn wait_error() -> IqResponse {
        IqResponse::Error(StanzaError::new(
            ErrorType::Wait,
            DefinedCondition::ResourceConstraint,
            "en",
            "busy",
        ))
    }

    #[test]
    fn a_temporary_error_retries_with_a_growing_delay_and_gives_up() {
        let mut h = ready();
        let start = Cursor {
            newest_id: Some("a5".into()),
            ..Cursor::default()
        };
        set_cursor(&mut h, "", start);
        h.with_ctx(on_connected);
        h.sent_iqs();
        let mut waits = Vec::new();
        for _ in 0..RETRY_TICKS.len() {
            h.respond(is_mam, wait_error());
            assert!(h.sent_iqs().is_empty());
            let mut ticks = 0;
            loop {
                h.with_ctx(on_tick);
                ticks += 1;
                if !h.sent_iqs().is_empty() {
                    break;
                }
                assert!(ticks < 100, "no retry");
            }
            waits.push(ticks);
            // The retry asks the same thing.
            let running = h.state.mam.queries.values().next().unwrap();
            assert_eq!(running.after.as_deref(), Some("a5"));
        }
        assert_eq!(waits, RETRY_TICKS);
        // The fourth failure is the last.
        h.respond(is_mam, wait_error());
        for _ in 0..100 {
            h.with_ctx(on_tick);
        }
        assert!(h.sent_iqs().is_empty());
        assert!(h.state.mam.queries.is_empty());
        assert!(h.state.mam.retries.is_empty());
    }

    #[test]
    fn a_permanent_error_does_not_retry() {
        let mut h = ready();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.respond(is_mam, error(DefinedCondition::ServiceUnavailable));
        assert!(h.state.mam.retries.is_empty());
    }

    #[test]
    fn an_answer_that_does_not_parse_retries() {
        let mut h = ready();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.answer(is_mam, None);
        assert_eq!(h.state.mam.retries.len(), 1);
    }

    #[test]
    fn a_live_stanza_id_moves_the_cursor_after_the_catch_up() {
        let mut h = ready();
        let start = Cursor {
            newest_id: Some("a5".into()),
            ..Cursor::default()
        };
        set_cursor(&mut h, "", start);
        h.with_ctx(on_connected);
        h.sent_iqs();
        // The catch-up runs: a live message does not move the cursor.
        h.with_ctx(|ctx| on_live(ctx, "a7"));
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a5"));
        h.answer(is_mam, Some(fin(true, Some(("a6", None)), Some("a6"))));
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a6"));
        h.with_ctx(|ctx| on_live(ctx, "a7"));
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a7"));
    }

    #[test]
    fn a_live_message_with_a_stanza_id_of_the_account_moves_the_cursor() {
        let mut h = ready();
        h.with_ctx(on_connected);
        h.sent_iqs();
        h.answer(is_mam, Some(fin(true, Some(("a1", Some(0))), Some("a2"))));
        let mut m = Message::chat(Jid::new("alice@chord.localhost").unwrap())
            .with_body("".into(), "live".into());
        m.from = Some(Jid::new("bob@chord.localhost/phone").unwrap());
        m.payloads.push(
            "<stanza-id xmlns='urn:xmpp:sid:0' id='a3' by='alice@chord.localhost'/>"
                .parse()
                .unwrap(),
        );
        h.with_ctx(|ctx| chat::on_message(ctx, &m));
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a3"));
        // A stanza-id of another entity does not count.
        let mut m = Message::chat(Jid::new("alice@chord.localhost").unwrap())
            .with_body("".into(), "forged".into());
        m.from = Some(Jid::new("bob@chord.localhost/phone").unwrap());
        m.payloads.push(
            "<stanza-id xmlns='urn:xmpp:sid:0' id='zz' by='bob@chord.localhost'/>"
                .parse()
                .unwrap(),
        );
        h.with_ctx(|ctx| chat::on_message(ctx, &m));
        assert_eq!(cursor(&mut h, "").newest_id.as_deref(), Some("a3"));
    }

    #[test]
    fn commands_answer_and_offline_fails() {
        let mut h = ready();
        let (reply, mut answer) = oneshot::channel();
        h.with_ctx(|ctx| on_command(ctx, Command::CatchUp { reply }));
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
        assert_eq!(h.sent_iqs().len(), 1);

        let (reply, mut answer) = oneshot::channel();
        offline(Command::Older {
            peer: room(),
            reply,
        });
        assert_eq!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::NotConnected))
        );
    }
}
