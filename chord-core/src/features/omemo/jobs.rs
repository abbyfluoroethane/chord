//! The work that waits for device lists and bundles: a send, or a refresh that the user
//! asked for.
//!
//! A job names the accounts that it needs. `advance` finds what is still missing: first the
//! device lists, then the bundles of the devices that need one. It sends the requests that
//! are not out yet, and the job waits. When no request is left, the job runs: the send
//! encrypts and goes out, the refresh answers.
//!
//! Each IQ ends in an answer, an error, or a time-out error (the actor fails an IQ after
//! about a minute), so a job never waits for ever. A device with no bundle is left out, so
//! the message goes to the devices that answered. A lost connection fails the jobs.

use std::collections::HashSet;

use jid::BareJid;
use xmpp_parsers::message::Message;

use super::store;
use super::trust;
use super::xml;
use super::{Pending, Reply, device, send};
use crate::actor::{ClientError, ClientEvent};
use crate::features::{Ctx, IqResponse, Pending as FeaturePending};

/// A device list that we read less than this long ago (5 minutes) is fresh. PEP events keep
/// it current after that, but a contact who does not share presence sends none.
const LIST_FRESH_MS: i64 = 5 * 60 * 1000;

/// One request that a job waits for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Wait {
    List(BareJid),
    Bundle(BareJid, u32),
}

/// What to do when a change of a message (a correction, a retraction, a reaction) is sent.
pub(crate) type OnSent = Box<dyn FnOnce(&mut Ctx<'_>) + Send>;

/// A message that waits to be encrypted.
pub(super) struct Held {
    pub message: Message,
    pub peer: BareJid,
    /// For a change of an earlier message: the id of that message, and the local change.
    /// The local change runs only when the message goes out. If the send fails, nothing
    /// changed in the store, and the failure names the target.
    pub change: Option<Change>,
}

pub(crate) struct Change {
    pub target_id: String,
    pub on_sent: OnSent,
}

pub(super) enum Kind {
    Send(Box<Held>),
    /// Read the lists and the bundles. `None` is a refresh that no one waits for.
    Refresh(Option<Reply<()>>),
}

pub(super) struct Job {
    id: u64,
    peers: Vec<BareJid>,
    kind: Kind,
    /// The requests that this job has seen answered.
    done: HashSet<Wait>,
    /// The requests that this job waits for now.
    needed: HashSet<Wait>,
}

/// Start a job that sends `message` to `peer`.
pub(super) fn submit_send(
    ctx: &mut Ctx<'_>,
    message: Message,
    peer: BareJid,
    change: Option<Change>,
) {
    let own = ctx.account.clone();
    add(
        ctx,
        vec![peer.clone(), own],
        Kind::Send(Box::new(Held {
            message,
            peer,
            change,
        })),
    );
}

/// Start a refresh of the devices of `jid`.
pub(super) fn refresh(ctx: &mut Ctx<'_>, jid: BareJid, reply: Option<Reply<()>>) {
    if ctx.state.omemo.engine.is_none() {
        if let Some(reply) = reply {
            let text = ctx.state.omemo.problem.clone().unwrap_or_default();
            let _ = reply.send(Err(ClientError::Invalid(format!(
                "OMEMO is not ready: {text}"
            ))));
        }
        return;
    }
    add(ctx, vec![jid], Kind::Refresh(reply));
}

fn add(ctx: &mut Ctx<'_>, peers: Vec<BareJid>, kind: Kind) {
    let state = &mut ctx.state.omemo;
    let id = state.next_job;
    state.next_job += 1;
    state.jobs.push(Job {
        id,
        peers,
        kind,
        done: HashSet::new(),
        needed: HashSet::new(),
    });
    advance(ctx, id);
}

/// Ask for the device list of `jid`, unless a request is out already.
pub(super) fn request_list(ctx: &mut Ctx<'_>, jid: &BareJid) {
    if !ctx.state.omemo.in_flight.insert(Wait::List(jid.clone())) {
        return;
    }
    let to = (*jid != *ctx.account).then_some(jid);
    ctx.request(
        xml::fetch_devices_iq(to),
        FeaturePending::Omemo(Pending::List { jid: jid.clone() }),
    );
}

fn request_bundle(ctx: &mut Ctx<'_>, jid: &BareJid, device_id: u32) {
    if !ctx
        .state
        .omemo
        .in_flight
        .insert(Wait::Bundle(jid.clone(), device_id))
    {
        return;
    }
    let to = (*jid != *ctx.account).then_some(jid);
    ctx.request(
        xml::fetch_bundle_iq(to, device_id),
        FeaturePending::Omemo(Pending::Bundle {
            jid: jid.clone(),
            device: device_id,
        }),
    );
}

/// What a job still needs.
fn required(ctx: &Ctx<'_>, job: &Job) -> Vec<Wait> {
    let now = super::now_ms(ctx.store);
    let refresh = matches!(job.kind, Kind::Refresh(_));
    let mut lists = Vec::new();
    for jid in &job.peers {
        let wait = Wait::List(jid.clone());
        if job.done.contains(&wait) {
            continue;
        }
        let fresh = !refresh
            && ctx
                .state
                .omemo
                .listed
                .get(jid)
                .is_some_and(|t| now - t < LIST_FRESH_MS);
        if !fresh {
            lists.push(wait);
        }
    }
    if !lists.is_empty() {
        return lists;
    }
    let Some(engine) = &ctx.state.omemo.engine else {
        return Vec::new();
    };
    let this = engine.keys.device_id();
    let mut bundles = Vec::new();
    for jid in &job.peers {
        let rows = store::list_devices(ctx.store, ctx.account_id, jid.as_str()).unwrap_or_default();
        for row in rows {
            if !row.active || (*jid == *ctx.account && row.device_id == this) {
                continue;
            }
            let wait = Wait::Bundle(jid.clone(), row.device_id);
            if job.done.contains(&wait)
                || ctx
                    .state
                    .omemo
                    .bundles
                    .contains_key(&(jid.clone(), row.device_id))
            {
                continue;
            }
            let need = match &row.identity_key {
                // We must see the key to know the trust.
                None => true,
                Some(ik) => {
                    !refresh
                        && trust::level_for(ctx.store, ctx.account_id, jid.as_str(), ik)
                            .is_ok_and(trust::usable)
                        && store::load_session(
                            ctx.store,
                            &engine.sealer,
                            ctx.account_id,
                            jid.as_str(),
                            row.device_id,
                        )
                        .ok()
                        .flatten()
                        .is_none()
                }
            };
            if need {
                bundles.push(wait);
            }
        }
    }
    bundles
}

/// Send the requests that a job needs, or run the job if it needs none.
fn advance(ctx: &mut Ctx<'_>, id: u64) {
    let Some(pos) = ctx.state.omemo.jobs.iter().position(|j| j.id == id) else {
        return;
    };
    let needed = required(ctx, &ctx.state.omemo.jobs[pos]);
    if needed.is_empty() {
        let job = ctx.state.omemo.jobs.remove(pos);
        finish(ctx, job);
        return;
    }
    for wait in &needed {
        match wait {
            Wait::List(jid) => request_list(ctx, jid),
            Wait::Bundle(jid, device) => request_bundle(ctx, jid, *device),
        }
    }
    ctx.state.omemo.jobs[pos].needed = needed.into_iter().collect();
}

fn finish(ctx: &mut Ctx<'_>, job: Job) {
    match job.kind {
        Kind::Refresh(reply) => {
            if let Some(reply) = reply {
                let _ = reply.send(Ok(()));
            }
        }
        Kind::Send(held) => send::encrypt_and_send(ctx, *held),
    }
}

/// An answer came for `wait`. Tell the jobs that wait for it.
fn wake(ctx: &mut Ctx<'_>, wait: &Wait) {
    let ids: Vec<u64> = ctx
        .state
        .omemo
        .jobs
        .iter_mut()
        .filter(|j| j.needed.contains(wait))
        .map(|j| {
            j.done.insert(wait.clone());
            j.id
        })
        .collect();
    for id in ids {
        advance(ctx, id);
    }
}

/// The connection is lost. Fail every job.
pub(super) fn fail_all(ctx: &mut Ctx<'_>, reason: &str) {
    ctx.state.omemo.in_flight.clear();
    for job in std::mem::take(&mut ctx.state.omemo.jobs) {
        match job.kind {
            Kind::Send(held) => send::fail_held(ctx, *held, reason),
            Kind::Refresh(reply) => {
                if let Some(reply) = reply {
                    let _ = reply.send(Err(ClientError::NotConnected));
                }
            }
        }
    }
}

/// The answer to the request for a device list.
pub(super) fn on_list(ctx: &mut Ctx<'_>, jid: BareJid, response: IqResponse) {
    let wait = Wait::List(jid.clone());
    ctx.state.omemo.in_flight.remove(&wait);
    if matches!(response, IqResponse::Lost) {
        fail_all(ctx, "the connection was lost");
        return;
    }
    let list = device::list_of(&response);
    if let Some(list) = &list {
        if device::apply_list(ctx, &jid, list) {
            ctx.emit(ClientEvent::OmemoDevicesChanged(jid.clone()));
        }
        let now = super::now_ms(ctx.store);
        ctx.state.omemo.listed.insert(jid.clone(), now);
    } else if let IqResponse::Error(e) = &response {
        log::info!("no OMEMO device list of {jid}: {:?}", e.defined_condition);
    }
    if jid == *ctx.account {
        device::after_own_list(ctx, list);
    }
    wake(ctx, &wait);
}

/// The answer to the request for a bundle.
pub(super) fn on_bundle(ctx: &mut Ctx<'_>, jid: BareJid, device_id: u32, response: IqResponse) {
    let wait = Wait::Bundle(jid.clone(), device_id);
    ctx.state.omemo.in_flight.remove(&wait);
    match response {
        IqResponse::Lost => {
            fail_all(ctx, "the connection was lost");
            return;
        }
        IqResponse::Result(Some(payload)) => match xml::parse_bundle_result(&payload) {
            Ok(bundle) => {
                trust::learn_key(ctx, &jid, device_id, &bundle.ik);
                ctx.state
                    .omemo
                    .bundles
                    .insert((jid.clone(), device_id), bundle);
            }
            Err(e) => log::info!("bad OMEMO bundle of {jid} device {device_id}: {e}"),
        },
        IqResponse::Result(None) => {}
        IqResponse::Error(e) => {
            log::info!(
                "no OMEMO bundle of {jid} device {device_id}: {:?}",
                e.defined_condition
            );
        }
    }
    wake(ctx, &wait);
}
