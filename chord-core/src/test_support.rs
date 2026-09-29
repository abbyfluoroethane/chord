//! Test helpers: a fake `Session` with scripted events, and a small executor.

use core::future::Future;
use core::pin::{Pin, pin};
use core::task::{Context, Poll, Waker};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use futures_core::Stream;
use xmpp_parsers::stanza::Stanza;

use crate::session::{ConnectError, Session, SessionConfig, SessionError, SessionEvent};

/// A `Session` that returns scripted events and records the stanzas it gets.
pub struct FakeSession {
    events: Option<FakeEvents>,
    sent: Rc<RefCell<Vec<Stanza>>>,
    closed: bool,
}

/// The scripted event stream. It ends after the last event.
pub struct FakeEvents(VecDeque<SessionEvent>);

impl Stream for FakeEvents {
    type Item = SessionEvent;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<SessionEvent>> {
        Poll::Ready(self.0.pop_front())
    }
}

impl FakeSession {
    /// A session that returns `events` in order.
    pub fn scripted(events: Vec<SessionEvent>) -> Self {
        Self {
            events: Some(FakeEvents(events.into())),
            sent: Rc::default(),
            closed: false,
        }
    }

    /// A handle to the stanzas that `send` got. It stays valid after `disconnect`.
    pub fn sent(&self) -> Rc<RefCell<Vec<Stanza>>> {
        Rc::clone(&self.sent)
    }

    /// Close the fake, so that the next `send` fails.
    pub fn close(&mut self) {
        self.closed = true;
    }
}

impl Session for FakeSession {
    type Events = FakeEvents;

    async fn connect(_config: SessionConfig) -> Result<Self, ConnectError> {
        Ok(Self::scripted(Vec::new()))
    }

    async fn send(&self, stanza: Stanza) -> Result<(), SessionError> {
        if self.closed {
            return Err(SessionError::Closed);
        }
        self.sent.borrow_mut().push(stanza);
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

/// Get the next item of a stream.
pub async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    core::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
}
