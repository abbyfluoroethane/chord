//! Test harness for feature functions: an in-memory store and a `Ctx` with no session.

use std::collections::{HashMap, HashSet};

use jid::BareJid;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::stanza::Stanza;

use super::{Ctx, Effect, FeatureState, IqResponse, Pending, PendingIq, on_answer};
use crate::store::{Store, queries};
use crate::views::ViewKey;

pub(crate) const ACCOUNT: &str = "alice@chord.localhost";

pub(crate) struct Harness {
    pub store: Store,
    pub account: BareJid,
    pub account_id: i64,
    pub state: FeatureState,
    pub effects: Vec<Effect>,
    pub pending: HashMap<String, PendingIq>,
    pub dirty: HashSet<ViewKey>,
}

impl Harness {
    pub fn new() -> Self {
        let store = Store::open_in_memory().unwrap();
        let account_id = queries::ensure_account(store.conn(), ACCOUNT).unwrap();
        Self {
            store,
            account: BareJid::new(ACCOUNT).unwrap(),
            account_id,
            state: FeatureState::default(),
            effects: Vec::new(),
            pending: HashMap::new(),
            dirty: HashSet::new(),
        }
    }

    /// Call a feature function with a `Ctx`.
    pub fn with_ctx<R>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> R) -> R {
        let mut ctx = Ctx {
            store: &self.store,
            account: &self.account,
            account_id: self.account_id,
            state: &mut self.state,
            effects: &mut self.effects,
            pending: &mut self.pending,
            dirty: &mut self.dirty,
        };
        f(&mut ctx)
    }

    /// Take the stanzas that the features sent so far.
    pub fn take_sent(&mut self) -> Vec<Stanza> {
        let effects = std::mem::take(&mut self.effects);
        effects
            .into_iter()
            .filter_map(|e| match e {
                Effect::Send(s) => Some(*s),
                other => {
                    self.effects.push(other);
                    None
                }
            })
            .collect()
    }

    /// Take the IQs that the features sent so far.
    pub fn sent_iqs(&mut self) -> Vec<Iq> {
        self.take_sent()
            .into_iter()
            .filter_map(|s| match s {
                Stanza::Iq(iq) => Some(iq),
                _ => None,
            })
            .collect()
    }

    /// Answer the first pending IQ that matches `which` with a result.
    pub fn answer(&mut self, which: impl Fn(&Pending) -> bool, payload: Option<Element>) {
        self.respond(which, IqResponse::Result(payload));
    }

    /// Answer the first pending IQ that matches `which`.
    pub fn respond(&mut self, which: impl Fn(&Pending) -> bool, response: IqResponse) {
        let id = self
            .pending
            .iter()
            .find(|(_, p)| which(&p.then))
            .map(|(id, _)| id.clone())
            .expect("no matching pending IQ");
        let pending = self.pending.remove(&id).unwrap();
        self.with_ctx(|ctx| on_answer(ctx, pending, response));
    }

    /// Take the views marked as changed.
    pub fn take_dirty(&mut self) -> HashSet<ViewKey> {
        std::mem::take(&mut self.dirty)
    }
}
