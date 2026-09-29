//! `ChordClient`: the object that the foreign code holds. It owns a tokio runtime and the
//! actor of `chord-core`.

use core::future::Future;
use core::pin::Pin;
use std::sync::{Arc, Mutex};

use chord_core::actor::{self, ClientError, ClientHandle};
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig};
use chord_core::store::Store;
use chord_core::views::{ListDiff, ViewStream};
use futures_core::Stream;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::AbortHandle;

use crate::error::{ChordError, parse_bare, parse_jid};
use crate::types::*;

/// The size of the event queue for each event listener.
const EVENT_QUEUE: usize = 256;

// ---- Listeners ----

/// Receives the events of the client. Implement it in the foreign language. The client
/// calls it from a background thread.
#[uniffi::export(with_foreign)]
pub trait ClientEventListener: Send + Sync {
    fn on_event(&self, event: ClientEvent);
}

/// Receives the diffs of a timeline. The first diff is a `Reset` with the current list.
#[uniffi::export(with_foreign)]
pub trait TimelineListener: Send + Sync {
    fn on_diff(&self, diff: TimelineDiff);
}

/// Receives the diffs of a channel list. The first diff is a `Reset`.
#[uniffi::export(with_foreign)]
pub trait ChannelListListener: Send + Sync {
    fn on_diff(&self, diff: ChannelDiff);
}

/// Receives the diffs of the space list. The first diff is a `Reset`.
#[uniffi::export(with_foreign)]
pub trait SpaceListListener: Send + Sync {
    fn on_diff(&self, diff: SpaceDiff);
}

/// Receives the diffs of a member list. The first diff is a `Reset`.
#[uniffi::export(with_foreign)]
pub trait MemberListListener: Send + Sync {
    fn on_diff(&self, diff: MemberDiff);
}

// ---- Subscription ----

/// A running listener task. Call `cancel`, or drop the object, to stop the task.
#[derive(uniffi::Object)]
pub struct Subscription {
    task: Mutex<Option<AbortHandle>>,
}

impl Subscription {
    fn new(task: AbortHandle) -> Self {
        Self {
            task: Mutex::new(Some(task)),
        }
    }

    fn stop(&self) {
        let task = self.task.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(task) = task {
            task.abort();
        }
    }
}

#[uniffi::export]
impl Subscription {
    /// Stop the listener. It gets no more calls after this returns and its current call ends.
    pub fn cancel(&self) {
        self.stop();
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stop();
    }
}

type PaginateRequest = (u32, oneshot::Sender<Result<(), ChordError>>);

/// A timeline subscription. It also loads older messages.
#[derive(uniffi::Object)]
pub struct TimelineSubscription {
    sub: Subscription,
    requests: mpsc::UnboundedSender<PaginateRequest>,
}

#[uniffi::export(async_runtime = "tokio")]
impl TimelineSubscription {
    /// Stop the listener.
    pub fn cancel(&self) {
        self.sub.stop();
    }

    /// Show `count` older messages. If the store has fewer, the archive (MAM) fetches
    /// them. The new items arrive as diffs.
    pub async fn paginate_back(&self, count: u32) -> Result<(), ChordError> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send((count, reply))
            .map_err(|_| ChordError::ActorGone)?;
        answer.await.map_err(|_| ChordError::ActorGone)?
    }
}

// ---- Runtime ----

/// Owns the runtime. A plain drop of a runtime panics inside an async context, so this
/// guard shuts it down without waiting.
struct RuntimeGuard(Option<Runtime>);

impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Some(rt) = self.0.take() {
            rt.shutdown_background();
        }
    }
}

async fn next_item<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    core::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}

/// Parse the server string of `login`.
///
/// - `""` or `"srv"`: find the server with SRV records.
/// - `"starttls://host:port"`, `"host:port"` and `"host"`: STARTTLS. The default port is 5222.
///
/// `chord-core` has no direct TLS mode, so `tls://` is an error. Plain TCP is an error too.
pub(crate) fn parse_server(s: &str) -> Result<ServerAddr, ChordError> {
    let bad = |detail: String| ChordError::InvalidServer { detail };
    let s = s.trim();
    if s.is_empty() || s == "srv" {
        return Ok(ServerAddr::Srv);
    }
    let (scheme, rest) = s.split_once("://").unwrap_or(("starttls", s));
    match scheme {
        "starttls" => {}
        "tls" => return Err(bad(format!("direct TLS is not supported: {s}"))),
        "tcp" => return Err(bad(format!("plain TCP is not allowed: {s}"))),
        _ => return Err(bad(format!("unknown scheme in {s}"))),
    }
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) => (
            host,
            port.parse::<u16>()
                .map_err(|_| bad(format!("bad port in {s}")))?,
        ),
        None => (rest, 5222),
    };
    if host.is_empty() {
        return Err(bad(format!("no host in {s}")));
    }
    Ok(ServerAddr::StartTls {
        host: host.to_owned(),
        port,
    })
}

// ---- The client ----

/// The XMPP client. Create one per account.
#[derive(uniffi::Object)]
pub struct ChordClient {
    handle: ClientHandle,
    rt: Handle,
    events: broadcast::Sender<ClientEvent>,
    account: String,
    // Last: the runtime stops after the other fields are gone.
    _guard: RuntimeGuard,
}

#[uniffi::export]
impl ChordClient {
    /// Open the database at `db_path` (created if it does not exist) and start the client
    /// for `account`. It does not connect: call `login`. The views work offline.
    #[uniffi::constructor]
    pub fn new(db_path: String, account: String) -> Result<Arc<Self>, ChordError> {
        let account_jid = parse_bare(&account)?;
        let store = Store::open(&db_path)?;
        let rt = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("chord-ffi")
            .enable_all()
            .build()
            .map_err(|e| ChordError::Internal {
                detail: format!("cannot start the runtime: {e}"),
            })?;
        let (handle, mut events, actor) = actor::new::<NativeSession>(store, account_jid.clone())?;
        let rt_handle = rt.handle().clone();
        drop(rt_handle.spawn(actor.run()));

        // One task reads the event stream and shares each event with all listeners.
        let (event_tx, _) = broadcast::channel(EVENT_QUEUE);
        let fanout = event_tx.clone();
        drop(rt_handle.spawn(async move {
            while let Some(event) = next_item(&mut events).await {
                // An error only means that nobody listens.
                let _ = fanout.send(event.into());
            }
        }));

        Ok(Arc::new(Self {
            handle,
            rt: rt_handle,
            events: event_tx,
            account: account_jid.to_string(),
            _guard: RuntimeGuard(Some(rt)),
        }))
    }

    /// The bare JID of the account.
    pub fn account(&self) -> String {
        self.account.clone()
    }

    /// Call `listener` for each client event. Events from before this call are lost, so
    /// subscribe before `login`.
    pub fn subscribe_events(&self, listener: Arc<dyn ClientEventListener>) -> Arc<Subscription> {
        let mut events = self.events.subscribe();
        let task = self.rt.spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => listener.on_event(event),
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        listener.on_event(ClientEvent::Notice {
                            text: format!("{n} events were dropped"),
                        });
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Arc::new(Subscription::new(task.abort_handle()))
    }
}

impl ChordClient {
    /// Run a `ClientHandle` call on the client runtime.
    async fn call<T, Fut>(
        &self,
        f: impl FnOnce(ClientHandle) -> Fut + Send,
    ) -> Result<T, ChordError>
    where
        Fut: Future<Output = Result<T, ClientError>> + Send + 'static,
        T: Send + 'static,
    {
        let task = self.rt.spawn(f(self.handle.clone()));
        task.await
            .map_err(|e| ChordError::Internal {
                detail: e.to_string(),
            })?
            .map_err(Into::into)
    }

    /// Send the diffs of `stream` to `deliver`, in a task.
    fn spawn_view<T, D>(
        &self,
        mut stream: ViewStream<T>,
        deliver: impl Fn(D) + Send + 'static,
    ) -> Arc<Subscription>
    where
        T: Send + 'static,
        D: From<ListDiff<T>>,
    {
        let task = self.rt.spawn(async move {
            while let Some(diff) = next_item(&mut stream).await {
                deliver(diff.into());
            }
        });
        Arc::new(Subscription::new(task.abort_handle()))
    }

    async fn open_timeline(
        &self,
        room: String,
        nick: Option<String>,
        listener: Arc<dyn TimelineListener>,
    ) -> Result<Arc<TimelineSubscription>, ChordError> {
        let room = parse_bare(&room)?;
        let mut timeline = self
            .call(move |h| async move {
                match nick {
                    Some(nick) => h.private_timeline(room, nick).await,
                    None => h.timeline(room).await,
                }
            })
            .await?;
        let (requests, mut inbox) = mpsc::unbounded_channel::<PaginateRequest>();
        let task = self.rt.spawn(async move {
            loop {
                tokio::select! {
                    diff = next_item(&mut timeline.stream) => match diff {
                        Some(diff) => listener.on_diff(diff.into()),
                        None => break,
                    },
                    request = inbox.recv() => match request {
                        Some((count, reply)) => {
                            let result = timeline
                                .paginate_back(count as usize)
                                .map_err(ChordError::from);
                            let _ = reply.send(result);
                        }
                        // The foreign code dropped the subscription.
                        None => break,
                    },
                }
            }
        });
        Ok(Arc::new(TimelineSubscription {
            sub: Subscription::new(task.abort_handle()),
            requests,
        }))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ChordClient {
    // ---- Session ----

    /// Log in. It returns after the first login succeeds or fails. `jid` must be the
    /// account. `server` is "" or "srv" for SRV lookup, "starttls://host:port", or "host".
    pub async fn login(
        &self,
        jid: String,
        password: String,
        server: String,
    ) -> Result<(), ChordError> {
        let jid = parse_bare(&jid)?;
        if jid.as_str() != self.account {
            return Err(ChordError::Invalid {
                detail: format!("the login JID {jid} is not the account {}", self.account),
            });
        }
        let config = SessionConfig::new(jid, password, parse_server(&server)?);
        let task = self.rt.spawn({
            let handle = self.handle.clone();
            async move { handle.login(config).await }
        });
        task.await
            .map_err(|e| ChordError::Internal {
                detail: e.to_string(),
            })?
            .map_err(Into::into)
    }

    /// Log out. The server gets every queued stanza before the stream closes.
    pub async fn logout(&self) -> Result<(), ChordError> {
        let handle = self.handle.clone();
        self.rt
            .spawn(async move { handle.logout().await })
            .await
            .map_err(|e| ChordError::Internal {
                detail: e.to_string(),
            })
    }

    // ---- Views ----

    /// Follow the space list.
    pub async fn subscribe_space_list(
        &self,
        listener: Arc<dyn SpaceListListener>,
    ) -> Result<Arc<Subscription>, ChordError> {
        let stream = self.call(|h| async move { h.space_list().await }).await?;
        Ok(self.spawn_view(stream, move |d: SpaceDiff| listener.on_diff(d)))
    }

    /// Follow the channels of a space, or of Home.
    pub async fn subscribe_channel_list(
        &self,
        scope: ChannelScope,
        listener: Arc<dyn ChannelListListener>,
    ) -> Result<Arc<Subscription>, ChordError> {
        let scope = scope.into();
        let stream = self
            .call(move |h| async move { h.channel_list(scope).await })
            .await?;
        Ok(self.spawn_view(stream, move |d: ChannelDiff| listener.on_diff(d)))
    }

    /// Follow the members of a room, or both people of a 1:1 chat.
    pub async fn subscribe_member_list(
        &self,
        room: String,
        listener: Arc<dyn MemberListListener>,
    ) -> Result<Arc<Subscription>, ChordError> {
        let room = parse_bare(&room)?;
        let stream = self
            .call(move |h| async move { h.member_list(room).await })
            .await?;
        Ok(self.spawn_view(stream, move |d: MemberDiff| listener.on_diff(d)))
    }

    /// Follow the messages of a room or a 1:1 chat.
    pub async fn subscribe_timeline(
        &self,
        room: String,
        listener: Arc<dyn TimelineListener>,
    ) -> Result<Arc<TimelineSubscription>, ChordError> {
        self.open_timeline(room, None, listener).await
    }

    /// Follow the private messages with one occupant of a room.
    pub async fn subscribe_private_timeline(
        &self,
        room: String,
        nick: String,
        listener: Arc<dyn TimelineListener>,
    ) -> Result<Arc<TimelineSubscription>, ChordError> {
        self.open_timeline(room, Some(nick), listener).await
    }

    // ---- Messages ----

    /// Send a chat message and store it. Returns its origin-id.
    pub async fn send_chat(&self, to: String, body: String) -> Result<String, ChordError> {
        let to = parse_jid(&to)?;
        self.call(move |h| async move { h.send_chat(to, body).await })
            .await
    }

    /// Send a private message to one occupant of a room. Returns its origin-id.
    pub async fn send_private(
        &self,
        room: String,
        nick: String,
        body: String,
    ) -> Result<String, ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.send_private(room, nick, body).await })
            .await
    }

    /// Correct a message of ours (XEP-0308). `item_id` is the id of a timeline item.
    pub async fn edit_message(&self, item_id: String, body: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.edit_message(item_id, body).await })
            .await
    }

    /// Retract a message of ours (XEP-0424).
    pub async fn retract_message(&self, item_id: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.retract_message(item_id).await })
            .await
    }

    /// Reply to a message (XEP-0461).
    pub async fn reply(&self, item_id: String, body: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.reply(item_id, body).await })
            .await
    }

    /// Set our reactions to a message to exactly `emojis` (XEP-0444).
    pub async fn react(&self, item_id: String, emojis: Vec<String>) -> Result<(), ChordError> {
        self.call(move |h| async move { h.react(item_id, emojis).await })
            .await
    }

    /// Add our reaction `emoji` to a message, or remove it if it is there.
    pub async fn toggle_reaction(&self, item_id: String, emoji: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.toggle_reaction(item_id, emoji).await })
            .await
    }

    /// Mark a chat or room as read and tell the peer (XEP-0333).
    pub async fn mark_read(&self, peer: String) -> Result<(), ChordError> {
        let peer = parse_bare(&peer)?;
        self.call(move |h| async move { h.mark_read(peer).await })
            .await
    }

    /// Set the notification level of a chat, a room, or a room occupant
    /// (room@service/nick). `mute_until` is a Unix time in ms. Works offline.
    pub async fn set_notification_level(
        &self,
        peer: String,
        level: NotificationLevel,
        mute_until: Option<i64>,
    ) -> Result<(), ChordError> {
        self.call(move |h| async move {
            h.set_notification_level(peer, level.into(), mute_until)
                .await
        })
        .await
    }

    /// The notification level of a peer. Works offline.
    pub async fn notification_level(
        &self,
        peer: String,
    ) -> Result<NotificationSetting, ChordError> {
        let setting = self
            .call(move |h| async move { h.notification_level(peer).await })
            .await?;
        Ok(setting.into())
    }

    /// Mark a private chat with a room occupant as read.
    pub async fn mark_read_private(&self, room: String, nick: String) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.mark_read_private(room, nick).await })
            .await
    }

    /// Ask the room to retract a message of another occupant (XEP-0425). We must be a
    /// room moderator. The row changes when the room announces the retraction.
    pub async fn moderate_message(
        &self,
        item_id: String,
        reason: Option<String>,
    ) -> Result<(), ChordError> {
        self.call(move |h| async move { h.moderate_message(item_id, reason).await })
            .await
    }

    // ---- Rooms and bookmarks ----

    /// Join a room.
    pub async fn join_room(
        &self,
        room: String,
        nick: String,
        password: Option<String>,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.join_room(room, nick, password).await })
            .await
    }

    /// Set the affiliation of a JID with a room. We need the right to do it.
    pub async fn set_room_affiliation(
        &self,
        room: String,
        jid: String,
        affiliation: RoomAffiliation,
        reason: Option<String>,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move {
            h.set_room_affiliation(room, jid, affiliation.into(), reason)
                .await
        })
        .await
    }

    /// List the JIDs with one affiliation in a room.
    pub async fn room_affiliations(
        &self,
        room: String,
        affiliation: RoomAffiliation,
    ) -> Result<Vec<RoomMember>, ChordError> {
        let room = parse_bare(&room)?;
        let list = self
            .call(move |h| async move { h.room_affiliations(room, affiliation.into()).await })
            .await?;
        Ok(list
            .into_iter()
            .map(|(jid, nick)| RoomMember {
                jid: jid.to_string(),
                nick,
            })
            .collect())
    }

    /// Invite a JID to a room. An owner or admin makes the JID a member first.
    pub async fn invite_to_room(
        &self,
        room: String,
        jid: String,
        reason: Option<String>,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move { h.invite_to_room(room, jid, reason).await })
            .await
    }

    /// Decline an invitation that a `RoomInvite` event reported.
    pub async fn decline_room_invite(
        &self,
        room: String,
        from: String,
        reason: Option<String>,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        let from = parse_bare(&from)?;
        self.call(move |h| async move { h.decline_room_invite(room, from, reason).await })
            .await
    }

    /// Change the settings of a room that we own.
    pub async fn configure_room(
        &self,
        room: String,
        settings: RoomSettings,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.configure_room(room, settings.into()).await })
            .await
    }

    /// Leave a room.
    pub async fn leave_room(&self, room: String) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.leave_room(room).await })
            .await
    }

    /// Change our nick in a room that we are in.
    pub async fn change_nick(&self, room: String, nick: String) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.change_nick(room, nick).await })
            .await
    }

    /// Add or update a bookmark.
    pub async fn add_bookmark(
        &self,
        room: String,
        name: Option<String>,
        autojoin: bool,
        nick: Option<String>,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.add_bookmark(room, name, autojoin, nick).await })
            .await
    }

    /// Remove a bookmark.
    pub async fn remove_bookmark(&self, room: String) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.remove_bookmark(room).await })
            .await
    }

    // ---- Roster ----

    /// Add a contact and ask to see its presence.
    pub async fn add_contact(&self, jid: String, name: Option<String>) -> Result<(), ChordError> {
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move { h.add_contact(jid, name).await })
            .await
    }

    /// Remove a contact from the roster.
    pub async fn remove_contact(&self, jid: String) -> Result<(), ChordError> {
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move { h.remove_contact(jid).await })
            .await
    }

    /// Accept the subscription request of a contact.
    pub async fn approve_subscription(&self, jid: String) -> Result<(), ChordError> {
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move { h.approve_subscription(jid).await })
            .await
    }

    /// Refuse the subscription request of a contact.
    pub async fn deny_subscription(&self, jid: String) -> Result<(), ChordError> {
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move { h.deny_subscription(jid).await })
            .await
    }

    /// Accept the future subscription request of a contact in advance.
    pub async fn preapprove_subscription(&self, jid: String) -> Result<(), ChordError> {
        let jid = parse_bare(&jid)?;
        self.call(move |h| async move { h.preapprove_subscription(jid).await })
            .await
    }

    /// The contacts of the roster. Works offline, from the store.
    pub async fn contacts(&self) -> Result<Vec<Contact>, ChordError> {
        let contacts = self.call(|h| async move { h.contacts().await }).await?;
        Ok(contacts.into_iter().map(Into::into).collect())
    }

    // ---- Archive ----

    /// Fetch the messages that arrived while we were offline (MAM).
    pub async fn sync_archive(&self) -> Result<(), ChordError> {
        self.call(|h| async move { h.sync_archive().await }).await
    }

    /// Fetch older messages of one chat or room from the archive.
    pub async fn load_older(&self, peer: String) -> Result<(), ChordError> {
        let peer = parse_bare(&peer)?;
        self.call(move |h| async move { h.load_older(peer).await })
            .await
    }

    // ---- Spaces ----

    /// List the spaces that the pubsub service offers.
    pub async fn browse_spaces(&self) -> Result<Vec<SpaceInfo>, ChordError> {
        let spaces = self
            .call(|h| async move { h.browse_spaces().await })
            .await?;
        Ok(spaces.into_iter().map(Into::into).collect())
    }

    /// Join a space.
    pub async fn join_space(
        &self,
        service: String,
        node: String,
    ) -> Result<JoinOutcome, ChordError> {
        let outcome = self
            .call(move |h| async move { h.join_space(&service, &node).await })
            .await?;
        Ok(outcome.into())
    }

    /// Leave a space.
    pub async fn leave_space(&self, service: String, node: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.leave_space(&service, &node).await })
            .await
    }

    /// Create a space. Returns its address.
    pub async fn create_space(&self, name: String, private: bool) -> Result<SpaceRef, ChordError> {
        let (service, node) = self
            .call(move |h| async move { h.create_space(&name, private).await })
            .await?;
        Ok(SpaceRef { service, node })
    }

    /// Create a space with an access model. `Authorize` needs service support.
    pub async fn create_space_with(
        &self,
        name: String,
        access: SpaceAccess,
    ) -> Result<SpaceRef, ChordError> {
        let (service, node) = self
            .call(move |h| async move { h.create_space_with(&name, access.into()).await })
            .await?;
        Ok(SpaceRef { service, node })
    }

    /// The spaces that we asked to join and that wait for the owner. Works offline.
    pub async fn pending_space_joins(&self) -> Result<Vec<PendingSpaceJoin>, ChordError> {
        let list = self
            .call(|h| async move { h.pending_space_joins().await })
            .await?;
        Ok(list
            .into_iter()
            .map(|(service, node, name)| PendingSpaceJoin {
                service,
                node,
                name,
            })
            .collect())
    }

    /// The join requests that wait for us, the owner of a space.
    pub async fn space_join_requests(
        &self,
        service: String,
        node: String,
    ) -> Result<Vec<JoinRequest>, ChordError> {
        let list = self
            .call(move |h| async move { h.space_join_requests(&service, &node).await })
            .await?;
        Ok(list.into_iter().map(Into::into).collect())
    }

    /// Approve a join request (owner only).
    pub async fn approve_space_join(
        &self,
        service: String,
        node: String,
        jid: String,
    ) -> Result<(), ChordError> {
        self.call(move |h| async move { h.approve_space_join(&service, &node, &jid).await })
            .await
    }

    /// Deny a join request (owner only).
    pub async fn deny_space_join(
        &self,
        service: String,
        node: String,
        jid: String,
    ) -> Result<(), ChordError> {
        self.call(move |h| async move { h.deny_space_join(&service, &node, &jid).await })
            .await
    }

    /// Add a room to a space.
    pub async fn add_room_to_space(
        &self,
        service: String,
        node: String,
        room: String,
        name: String,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.add_room_to_space(&service, &node, room, &name).await })
            .await
    }

    /// Remove a room from a space.
    pub async fn remove_room_from_space(
        &self,
        service: String,
        node: String,
        room: String,
    ) -> Result<(), ChordError> {
        let room = parse_bare(&room)?;
        self.call(move |h| async move { h.remove_room_from_space(&service, &node, room).await })
            .await
    }

    /// Add a member to a space.
    pub async fn add_space_member(
        &self,
        service: String,
        node: String,
        member: String,
    ) -> Result<(), ChordError> {
        let member = parse_bare(&member)?;
        self.call(move |h| async move { h.add_space_member(&service, &node, member).await })
            .await
    }

    /// Delete a space that we own.
    pub async fn delete_space(&self, service: String, node: String) -> Result<(), ChordError> {
        self.call(move |h| async move { h.delete_space(&service, &node).await })
            .await
    }

    // ---- Upload and avatars ----

    /// Upload a file (XEP-0363) and send its URL to `to`. Returns the URL.
    pub async fn upload(
        &self,
        to: String,
        filename: String,
        content_type: String,
        data: Vec<u8>,
    ) -> Result<String, ChordError> {
        let to = parse_jid(&to)?;
        self.call(move |h| async move { h.upload(to, filename, content_type, data).await })
            .await
    }

    /// The stored avatar of an account, a contact, or a room.
    pub async fn avatar(&self, owner: String) -> Result<Option<Avatar>, ChordError> {
        let owner = parse_bare(&owner)?;
        let avatar = self
            .call(move |h| async move { h.avatar(owner).await })
            .await?;
        Ok(avatar.map(Into::into))
    }

    /// Fetch the avatar of `owner` from the server again.
    pub async fn refresh_avatar(&self, owner: String) -> Result<(), ChordError> {
        let owner = parse_bare(&owner)?;
        self.call(move |h| async move { h.refresh_avatar(owner).await })
            .await
    }

    /// Remove the avatar of our account.
    pub async fn remove_avatar(&self) -> Result<(), ChordError> {
        self.call(|h| async move { h.remove_avatar().await }).await
    }

    /// Set the avatar of our account.
    pub async fn set_avatar(
        &self,
        mime: String,
        data: Vec<u8>,
        width: u16,
        height: u16,
    ) -> Result<(), ChordError> {
        self.call(move |h| async move { h.set_avatar(mime, data, width, height).await })
            .await
    }

    /// Set the photo of our vCard (XEP-0054, XEP-0153) only.
    pub async fn set_vcard_photo(&self, mime: String, data: Vec<u8>) -> Result<(), ChordError> {
        self.call(move |h| async move { h.set_vcard_photo(mime, data).await })
            .await
    }

    /// Remove the photo of our vCard only.
    pub async fn remove_vcard_photo(&self) -> Result<(), ChordError> {
        self.call(|h| async move { h.remove_vcard_photo().await })
            .await
    }

    // ---- Push ----

    /// Enable push notifications (XEP-0357). `form` holds the publish options.
    pub async fn enable_push(
        &self,
        service: String,
        node: String,
        form: Option<Vec<FormField>>,
    ) -> Result<(), ChordError> {
        let service = parse_jid(&service)?;
        let form = form.map(|f| f.into_iter().map(|f| (f.name, f.value)).collect());
        self.call(move |h| async move { h.enable_push(service, node, form).await })
            .await
    }

    /// Disable push notifications for a service, or for one node of it.
    pub async fn disable_push(
        &self,
        service: String,
        node: Option<String>,
    ) -> Result<(), ChordError> {
        let service = parse_jid(&service)?;
        self.call(move |h| async move { h.disable_push(service, node).await })
            .await
    }

    /// The push services that this account enabled.
    pub async fn push_registrations(&self) -> Result<Vec<PushRegistration>, ChordError> {
        let list = self
            .call(|h| async move { h.push_registrations().await })
            .await?;
        Ok(list.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use super::*;

    fn client() -> Arc<ChordClient> {
        ChordClient::new(":memory:".into(), "alice@example.org".into()).unwrap()
    }

    #[test]
    fn server_strings_parse() {
        assert_eq!(parse_server("").unwrap(), ServerAddr::Srv);
        assert_eq!(parse_server("srv").unwrap(), ServerAddr::Srv);
        let starttls = |host: &str, port| ServerAddr::StartTls {
            host: host.into(),
            port,
        };
        assert_eq!(
            parse_server("starttls://xmpp.example:5223").unwrap(),
            starttls("xmpp.example", 5223)
        );
        assert_eq!(
            parse_server("xmpp.example").unwrap(),
            starttls("xmpp.example", 5222)
        );
        assert_eq!(
            parse_server("xmpp.example:5000").unwrap(),
            starttls("xmpp.example", 5000)
        );
    }

    #[test]
    fn bad_server_strings_are_errors() {
        for bad in [
            "tls://x:5223",
            "tcp://x:5222",
            "http://x:80",
            "starttls://x:port",
            "starttls://:5222",
        ] {
            assert!(
                matches!(parse_server(bad), Err(ChordError::InvalidServer { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn bad_account_and_db_are_typed_errors() {
        assert!(matches!(
            ChordClient::new(":memory:".into(), "not a jid@".into()),
            Err(ChordError::InvalidJid { .. })
        ));
        assert!(matches!(
            ChordClient::new("/nonexistent-dir/x/db.sqlite3".into(), "a@b".into()),
            Err(ChordError::Store { .. })
        ));
    }

    #[tokio::test]
    async fn contacts_work_offline() {
        let client = client();
        assert_eq!(client.account(), "alice@example.org");
        assert!(client.contacts().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn bad_jids_return_a_typed_error_before_any_command() {
        let client = client();
        // A resource is not valid where a bare JID is needed.
        assert!(matches!(
            client
                .join_room("room@muc.example/nick".into(), "n".into(), None)
                .await,
            Err(ChordError::InvalidJid { .. })
        ));
        assert!(matches!(
            client.send_chat("@@".into(), "hi".into()).await,
            Err(ChordError::InvalidJid { .. })
        ));
        assert!(matches!(
            client
                .login("bob@example.org".into(), "pw".into(), "tls://x:1".into())
                .await,
            Err(ChordError::Invalid { .. })
        ));
        assert!(matches!(
            client
                .login("alice@example.org".into(), "pw".into(), "tls://x:1".into())
                .await,
            Err(ChordError::InvalidServer { .. })
        ));
    }

    #[tokio::test]
    async fn offline_commands_report_not_connected() {
        let client = client();
        let result = client.sync_archive().await;
        assert!(
            matches!(result, Err(ChordError::NotConnected)),
            "{result:?}"
        );
    }

    struct Collect(StdMutex<Vec<ChannelDiff>>);

    impl ChannelListListener for Collect {
        fn on_diff(&self, diff: ChannelDiff) {
            self.0.lock().unwrap().push(diff);
        }
    }

    struct Timeline(StdMutex<Vec<TimelineDiff>>);

    impl TimelineListener for Timeline {
        fn on_diff(&self, diff: TimelineDiff) {
            self.0.lock().unwrap().push(diff);
        }
    }

    #[tokio::test]
    async fn views_start_with_a_reset_and_stop_on_cancel() {
        let client = client();
        let channels = Arc::new(Collect(StdMutex::default()));
        let sub = client
            .subscribe_channel_list(ChannelScope::Home, channels.clone())
            .await
            .unwrap();
        let timeline = Arc::new(Timeline(StdMutex::default()));
        let tl = client
            .subscribe_timeline("room@muc.example".into(), timeline.clone())
            .await
            .unwrap();
        tl.paginate_back(10).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while channels.0.lock().unwrap().is_empty() || timeline.0.lock().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            channels.0.lock().unwrap()[0],
            ChannelDiff::Reset { items: vec![] }
        );
        assert_eq!(
            timeline.0.lock().unwrap()[0],
            TimelineDiff::Reset { items: vec![] }
        );
        sub.cancel();
        tl.cancel();
        // After cancel the task is gone, so a page request fails.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(matches!(
            tl.paginate_back(1).await,
            Err(ChordError::ActorGone)
        ));
    }
}
