//! Test helpers: a fake `Session` with scripted events, and two small executors.

use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use futures_core::Stream;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::stanza::Stanza;

use crate::session::{ConnectError, Session, SessionConfig, SessionError, SessionEvent};

type Queue = Rc<RefCell<VecDeque<SessionEvent>>>;

/// A `Session` that returns scripted events and records the stanzas it gets.
///
/// It also acts as a small server: it answers each `<iq type='get'/>` with an empty
/// result, so that pings work.
pub struct FakeSession {
    events: Option<FakeEvents>,
    queue: Queue,
    sent: Rc<RefCell<Vec<Stanza>>>,
    client_states: Rc<RefCell<Vec<bool>>>,
    closed: bool,
}

/// The scripted event stream.
pub struct FakeEvents {
    queue: Queue,
    /// End the stream when the queue is empty. Otherwise stay pending, like a live session.
    end_when_empty: bool,
}

impl Stream for FakeEvents {
    type Item = SessionEvent;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<SessionEvent>> {
        match self.queue.borrow_mut().pop_front() {
            Some(event) => Poll::Ready(Some(event)),
            None if self.end_when_empty => Poll::Ready(None),
            None => Poll::Pending,
        }
    }
}

/// What the next `FakeSession::connect` on this thread returns.
struct Prepared {
    result: Result<Vec<SessionEvent>, ConnectError>,
    sent: Rc<RefCell<Vec<Stanza>>>,
}

thread_local! {
    static NEXT_CONNECT: RefCell<Option<Prepared>> = const { RefCell::new(None) };
    /// The event queue of the last session that `connect` returned on this thread.
    static LIVE_QUEUE: RefCell<Option<Queue>> = const { RefCell::new(None) };
    /// The client states of the last session that `connect` returned on this thread.
    static LIVE_CLIENT_STATES: RefCell<Option<Rc<RefCell<Vec<bool>>>>> = const { RefCell::new(None) };
}

impl FakeSession {
    /// A session that returns `events` in order, then ends its event stream.
    pub fn scripted(events: Vec<SessionEvent>) -> Self {
        Self::new(events, Rc::default(), true)
    }

    fn new(
        events: Vec<SessionEvent>,
        sent: Rc<RefCell<Vec<Stanza>>>,
        end_when_empty: bool,
    ) -> Self {
        let queue: Queue = Rc::new(RefCell::new(events.into()));
        Self {
            events: Some(FakeEvents {
                queue: Rc::clone(&queue),
                end_when_empty,
            }),
            queue,
            sent,
            client_states: Rc::default(),
            closed: false,
        }
    }

    /// Make the next `connect` on this thread succeed with a live session that returns
    /// `events` and then stays open. Returns a handle to the stanzas it gets.
    pub fn prepare_connect(events: Vec<SessionEvent>) -> Rc<RefCell<Vec<Stanza>>> {
        let sent = Rc::<RefCell<Vec<Stanza>>>::default();
        let prepared = Prepared {
            result: Ok(events),
            sent: Rc::clone(&sent),
        };
        NEXT_CONNECT.with(|next| *next.borrow_mut() = Some(prepared));
        sent
    }

    /// Make the next `connect` on this thread fail with `error`.
    pub fn prepare_connect_error(error: ConnectError) {
        let prepared = Prepared {
            result: Err(error),
            sent: Rc::default(),
        };
        NEXT_CONNECT.with(|next| *next.borrow_mut() = Some(prepared));
    }

    /// Add an event to the live session that the last `connect` on this thread returned.
    pub fn push_event(event: SessionEvent) {
        LIVE_QUEUE.with(|live| {
            let live = live.borrow();
            let queue = live.as_ref().expect("no live FakeSession on this thread");
            queue.borrow_mut().push_back(event);
        });
    }

    /// A handle to the stanzas that `send` got. It stays valid after `disconnect`.
    pub fn sent(&self) -> Rc<RefCell<Vec<Stanza>>> {
        Rc::clone(&self.sent)
    }

    /// The client states (`true` is active) that the live session of the last `connect`
    /// on this thread got.
    pub fn live_client_states() -> Vec<bool> {
        LIVE_CLIENT_STATES.with(|live| {
            let live = live.borrow();
            let states = live.as_ref().expect("no live FakeSession on this thread");
            states.borrow().clone()
        })
    }

    /// Close the fake, so that the next `send` fails.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

impl Session for FakeSession {
    type Events = FakeEvents;

    async fn connect(_config: SessionConfig) -> Result<Self, ConnectError> {
        match NEXT_CONNECT.with(|next| next.borrow_mut().take()) {
            Some(Prepared {
                result: Ok(events),
                sent,
            }) => {
                let session = Self::new(events, sent, false);
                LIVE_QUEUE.with(|live| *live.borrow_mut() = Some(Rc::clone(&session.queue)));
                LIVE_CLIENT_STATES
                    .with(|live| *live.borrow_mut() = Some(Rc::clone(&session.client_states)));
                Ok(session)
            }
            Some(Prepared {
                result: Err(error), ..
            }) => Err(error),
            None => Ok(Self::scripted(Vec::new())),
        }
    }

    async fn send(&self, stanza: Stanza) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        if let Stanza::Iq(Iq::Get { id, from: None, .. }) = &stanza {
            let result = Iq::Result {
                from: None,
                to: None,
                id: id.clone(),
                payload: None,
            };
            let event = SessionEvent::Stanza(Box::new(result.into()));
            self.queue.borrow_mut().push_back(event);
        }
        self.sent.borrow_mut().push(stanza);
        Ok(())
    }

    async fn send_client_state(&self, active: bool) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        self.client_states.borrow_mut().push(active);
        Ok(())
    }

    fn events(&mut self) -> Option<FakeEvents> {
        self.events.take()
    }

    async fn disconnect(self) {}
}

/// Run a future that is ready at once. Panics if the future is pending.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    match fut.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("block_on: the future is pending. Fakes must be ready at once."),
    }
}

/// Poll `background` and `test` in turn until `test` is ready. Panics after many rounds,
/// so that a test that waits forever fails instead of hanging.
pub fn run_with<B: Future, T: Future>(background: B, test: T) -> T::Output {
    let mut background = pin!(background);
    let mut test = pin!(test);
    let mut background_done = false;
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..10_000 {
        if let Poll::Ready(value) = test.as_mut().poll(&mut cx) {
            return value;
        }
        if !background_done {
            background_done = background.as_mut().poll(&mut cx).is_ready();
        }
    }
    panic!("run_with: the test did not finish");
}

/// Get the next item of a stream.
pub async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    core::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}
