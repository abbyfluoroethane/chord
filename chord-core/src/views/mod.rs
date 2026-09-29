//! View-state models that the frontends render.
//!
//! Each view is a store query. A frontend subscribes to a view and gets a `Reset` with
//! the current list, then a diff each time the list changes. Features only write the
//! store and mark the views that changed (`Ctx::changed`). The actor then runs the query
//! of each subscribed view again and sends the diff.

pub mod channel_list;
pub mod diff;
pub mod member_list;
pub mod space_list;
pub mod timeline;

use core::pin::Pin;
use core::task::{Context, Poll};

use futures_channel::mpsc;
use futures_core::Stream;
use jid::BareJid;

use crate::store::Store;
pub use channel_list::{ChannelItem, ChannelKind, ChannelScope};
pub use diff::{ListDiff, ViewItem};
pub use member_list::MemberItem;
pub use space_list::SpaceItem;
pub use timeline::{DeliveryStatus, ReactionSummary, ReplyPreview, TimelineItem};

/// Messages that a timeline shows at first. `paginate_back` adds more.
pub const DEFAULT_TIMELINE_WINDOW: usize = 50;

/// Names a view. Features mark views as changed with it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ViewKey {
    SpaceList,
    ChannelList(ChannelScope),
    /// The messages of a room or of a 1:1 chat. The JID is the room or the peer.
    Timeline(BareJid),
    /// The private messages with one room occupant: the room and the nick.
    PrivateTimeline(BareJid, String),
    MemberList(BareJid),
    /// Every view. For a big change, for example after a new login.
    All,
}

/// The stream of diffs of one view subscription. Drop it to end the subscription.
pub struct ViewStream<T>(pub(crate) mpsc::UnboundedReceiver<ListDiff<T>>);

impl<T> Stream for ViewStream<T> {
    type Item = ListDiff<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<ListDiff<T>>> {
        Pin::new(&mut self.0).poll_next(cx)
    }
}

/// The data that the view queries need about the account.
pub(crate) struct QueryCtx<'a> {
    pub store: &'a Store,
    pub account_id: i64,
    pub account: &'a BareJid,
}

struct Sub<T> {
    sender: mpsc::UnboundedSender<ListDiff<T>>,
    last: Vec<T>,
}

impl<T: ViewItem> Sub<T> {
    /// Send the diff to the new list. Returns false if the subscriber is gone.
    fn update(&mut self, new: Vec<T>) -> bool {
        let diffs = diff::diff(&self.last, &new);
        self.last = new;
        diffs
            .into_iter()
            .all(|d| self.sender.unbounded_send(d).is_ok())
            && !self.sender.is_closed()
    }
}

enum Subscription {
    SpaceList(Sub<SpaceItem>),
    ChannelList(ChannelScope, Sub<ChannelItem>),
    Timeline {
        id: u64,
        room: BareJid,
        /// The nick of an occupant for a private timeline.
        nick: Option<String>,
        window: usize,
        sub: Sub<TimelineItem>,
    },
    MemberList(BareJid, Sub<MemberItem>),
}

impl Subscription {
    fn key(&self) -> ViewKey {
        match self {
            Self::SpaceList(_) => ViewKey::SpaceList,
            Self::ChannelList(scope, _) => ViewKey::ChannelList(scope.clone()),
            Self::Timeline {
                room, nick: None, ..
            } => ViewKey::Timeline(room.clone()),
            Self::Timeline {
                room,
                nick: Some(nick),
                ..
            } => ViewKey::PrivateTimeline(room.clone(), nick.clone()),
            Self::MemberList(room, _) => ViewKey::MemberList(room.clone()),
        }
    }

    /// Run the query again and send the diff. Returns false if the subscriber is gone.
    fn refresh(&mut self, q: &QueryCtx<'_>) -> bool {
        match self {
            Self::SpaceList(sub) => sub.update(log_err(space_list::query(q))),
            Self::ChannelList(scope, sub) => sub.update(log_err(channel_list::query(q, scope))),
            Self::Timeline {
                room,
                nick,
                window,
                sub,
                ..
            } => {
                let peer = timeline::peer_of(room, nick.as_deref());
                sub.update(log_err(timeline::query(q, &peer, *window)))
            }
            Self::MemberList(room, sub) => sub.update(log_err(member_list::query(q, room))),
        }
    }
}

fn log_err<T>(result: rusqlite::Result<Vec<T>>) -> Vec<T> {
    result.unwrap_or_else(|e| {
        log::error!("view query failed: {e}");
        Vec::new()
    })
}

/// All view subscriptions of the actor.
#[derive(Default)]
pub(crate) struct Registry {
    subs: Vec<Subscription>,
    next_id: u64,
}

impl Registry {
    pub fn subscribe_space_list(&mut self, q: &QueryCtx<'_>) -> ViewStream<SpaceItem> {
        let (sub, stream) = start(log_err(space_list::query(q)));
        self.subs.push(Subscription::SpaceList(sub));
        stream
    }

    pub fn subscribe_channel_list(
        &mut self,
        q: &QueryCtx<'_>,
        scope: ChannelScope,
    ) -> ViewStream<ChannelItem> {
        let (sub, stream) = start(log_err(channel_list::query(q, &scope)));
        self.subs.push(Subscription::ChannelList(scope, sub));
        stream
    }

    pub fn subscribe_member_list(
        &mut self,
        q: &QueryCtx<'_>,
        room: BareJid,
    ) -> ViewStream<MemberItem> {
        let (sub, stream) = start(log_err(member_list::query(q, &room)));
        self.subs.push(Subscription::MemberList(room, sub));
        stream
    }

    /// Returns the subscription id (for `paginate_back`) and the stream.
    pub fn subscribe_timeline(
        &mut self,
        q: &QueryCtx<'_>,
        room: BareJid,
        nick: Option<String>,
    ) -> (u64, ViewStream<TimelineItem>) {
        let window = DEFAULT_TIMELINE_WINDOW;
        let peer = timeline::peer_of(&room, nick.as_deref());
        let (sub, stream) = start(log_err(timeline::query(q, &peer, window)));
        self.next_id += 1;
        let id = self.next_id;
        self.subs.push(Subscription::Timeline {
            id,
            room,
            nick,
            window,
            sub,
        });
        (id, stream)
    }

    /// Show `count` more messages in a timeline. Returns the room if the store has fewer
    /// messages than the new window, so that MAM can fetch older ones.
    pub fn paginate_back(
        &mut self,
        q: &QueryCtx<'_>,
        timeline_id: u64,
        count: usize,
    ) -> Option<BareJid> {
        let sub = self
            .subs
            .iter_mut()
            .find(|s| matches!(s, Subscription::Timeline { id, .. } if *id == timeline_id))?;
        let Subscription::Timeline {
            room, nick, window, ..
        } = sub
        else {
            return None;
        };
        *window += count;
        let (room, window, private) = (room.clone(), *window, nick.is_some());
        sub.refresh(q);
        let Subscription::Timeline { sub: inner, .. } = sub else {
            return None;
        };
        // MAM has no archive for private messages.
        (inner.last.len() < window && !private).then_some(room)
    }

    /// Run the queries of the changed views and send the diffs. Drops the subscriptions
    /// whose subscribers are gone.
    pub fn refresh(&mut self, q: &QueryCtx<'_>, dirty: &std::collections::HashSet<ViewKey>) {
        if dirty.is_empty() {
            return;
        }
        let all = dirty.contains(&ViewKey::All);
        self.subs.retain_mut(|s| {
            if all || dirty.contains(&s.key()) {
                s.refresh(q)
            } else {
                !closed(s)
            }
        });
    }
}

fn closed(s: &Subscription) -> bool {
    match s {
        Subscription::SpaceList(sub) => sub.sender.is_closed(),
        Subscription::ChannelList(_, sub) => sub.sender.is_closed(),
        Subscription::Timeline { sub, .. } => sub.sender.is_closed(),
        Subscription::MemberList(_, sub) => sub.sender.is_closed(),
    }
}

fn start<T: ViewItem>(items: Vec<T>) -> (Sub<T>, ViewStream<T>) {
    let (sender, receiver) = mpsc::unbounded();
    let _ = sender.unbounded_send(ListDiff::Reset(items.clone()));
    (
        Sub {
            sender,
            last: items,
        },
        ViewStream(receiver),
    )
}
