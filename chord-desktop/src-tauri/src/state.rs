//! The state that Tauri manages: the open account, its subscriptions, and the event sink.
//!
//! One account at a time. `open` builds a `Client`. A second `open` replaces it, and the
//! `Drop` of the old one stops its tasks.

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};

use chord_core::actor::{ClientEvent, ClientHandle, Timeline};
use chord_core::jid::BareJid;
use chord_core::session::Stream;
use chord_core::store::Store;
use chord_core::views::{ListDiff, TimelineItem};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::ipc::Channel;

use crate::error::{ChordError, Res};

/// Lock a mutex. A panic in another task must not lock the app up for good.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Events that wait for the UI: kept while no `events` channel is set.
const EVENT_BACKLOG: usize = 200;

/// Where `ClientEvent`s go. The UI sets the channel with the `events` command.
#[derive(Default)]
pub struct EventSink {
    channel: Option<Channel<ClientEvent>>,
    backlog: VecDeque<ClientEvent>,
}

impl EventSink {
    /// Send the event to the UI, or keep it until the UI listens.
    pub fn push(&mut self, event: ClientEvent) {
        if let Some(channel) = &self.channel {
            if channel.send(event.clone()).is_ok() {
                return;
            }
            // The page reloaded: its channel is closed.
            self.channel = None;
        }
        if self.backlog.len() == EVENT_BACKLOG {
            self.backlog.pop_front();
        }
        self.backlog.push_back(event);
    }

    /// Use a new channel and send the events that waited.
    pub fn attach(&mut self, channel: Channel<ClientEvent>) {
        while let Some(event) = self.backlog.pop_front() {
            if channel.send(event).is_err() {
                break;
            }
        }
        self.channel = Some(channel);
    }
}

/// A view subscription: the task that forwards the diffs, and its timeline if it has one.
struct Sub {
    task: JoinHandle<()>,
    timeline: Option<Arc<Mutex<Timeline>>>,
}

/// The diff stream of a timeline. The `Timeline` stays in the map for `paginate_back`.
pub struct TimelineStream(pub Arc<Mutex<Timeline>>);

impl Stream for TimelineStream {
    type Item = ListDiff<TimelineItem>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // The lock is short: a poll never waits.
        Pin::new(&mut lock(&self.0).stream).poll_next(cx)
    }
}

/// The open account.
pub struct Client {
    pub handle: ClientHandle,
    pub account: BareJid,
    /// A second connection for the `chord-avatar` scheme, so that an image request never
    /// waits for the actor.
    pub avatars: Arc<Mutex<(Store, i64)>>,
    subs: Arc<Mutex<HashMap<u64, Sub>>>,
    next_sub: AtomicU64,
    /// The task that reads `ClientEvents`.
    pub events_task: JoinHandle<()>,
}

impl Client {
    pub fn new(
        handle: ClientHandle,
        account: BareJid,
        avatars: (Store, i64),
        events_task: JoinHandle<()>,
    ) -> Self {
        Self {
            handle,
            account,
            avatars: Arc::new(Mutex::new(avatars)),
            subs: Arc::default(),
            next_sub: AtomicU64::new(1),
            events_task,
        }
    }

    /// Forward the diffs of `stream` to `channel` until the UI or the view closes.
    /// Returns the subscription id.
    pub fn subscribe<T, S>(
        &self,
        mut stream: S,
        channel: Channel<ListDiff<T>>,
        timeline: Option<Arc<Mutex<Timeline>>>,
    ) -> u64
    where
        T: Serialize + Send + 'static,
        S: Stream<Item = ListDiff<T>> + Unpin + Send + 'static,
    {
        let id = self.next_sub.fetch_add(1, Ordering::Relaxed);
        let subs = Arc::clone(&self.subs);
        // Hold the lock from the spawn to the insert, so that the task cannot remove its
        // entry before the entry exists.
        let mut map = lock(&self.subs);
        let task = tauri::async_runtime::spawn(async move {
            while let Some(diff) =
                std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await
            {
                if channel.send(diff).is_err() {
                    break;
                }
            }
            lock(&subs).remove(&id);
        });
        map.insert(id, Sub { task, timeline });
        id
    }

    /// Stop a subscription. An unknown id is fine: the task may have ended already.
    pub fn unsubscribe(&self, id: u64) {
        if let Some(sub) = lock(&self.subs).remove(&id) {
            sub.task.abort();
        }
    }

    /// The timeline of a subscription, for `paginate_back`.
    pub fn timeline(&self, id: u64) -> Res<Arc<Mutex<Timeline>>> {
        lock(&self.subs)
            .get(&id)
            .and_then(|sub| sub.timeline.clone())
            .ok_or_else(|| ChordError::invalid(format!("no timeline subscription {id}")))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.events_task.abort();
        for (_, sub) in lock(&self.subs).drain() {
            sub.task.abort();
        }
    }
}

/// The managed state.
#[derive(Default)]
pub struct AppState {
    client: Mutex<Option<Arc<Client>>>,
    pub events: Arc<Mutex<EventSink>>,
}

impl AppState {
    pub fn client(&self) -> Res<Arc<Client>> {
        lock(&self.client).clone().ok_or_else(ChordError::not_open)
    }

    pub fn handle(&self) -> Res<ClientHandle> {
        Ok(self.client()?.handle.clone())
    }

    /// Install a client. Returns the old one, so that the caller drops it after the swap.
    pub fn replace(&self, client: Client) -> Option<Arc<Client>> {
        lock(&self.client).replace(Arc::new(client))
    }
}
