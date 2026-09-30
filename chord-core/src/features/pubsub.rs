//! Pubsub client (XEP-0060): requests, and the routing of event notifications.
//!
//! Routing of `<event/>` messages:
//! - node `urn:xmpp:bookmarks:1` from our own account: `bookmarks::on_event`,
//! - node `urn:chord:pins:0` from our own account: `pins::on_event`,
//! - node `urn:xmpp:mds:displayed:0` from our own account: `mds::on_event`,
//! - node `urn:xmpp:avatar:metadata` (PEP of any contact): `avatars::on_metadata_event`,
//! - node `http://jabber.org/protocol/tune` (PEP of any contact): `tune::on_event`,
//! - a data form `subscribe_authorization` from the pubsub service: `spaces::on_authorization`,
//! - anything else (a pubsub service, for example a space node): `spaces::on_event`.
//!
//! Publish with options (XEP-0060, 7.1.5): bookmarks, avatars, pins and MDS publish with
//! publish-options. A node that exists with another configuration makes the service answer
//! `conflict` (`precondition-not-met`). `Ctx::request_publish` keeps the publish. On that
//! answer, `on_answer` reads the node configuration, submits the wanted values and sends
//! the publish again, once. If any step fails, the feature gets the first error.

use jid::Jid;
use xmpp_parsers::data_forms::{DataForm, DataFormType};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::message::Message;
use xmpp_parsers::minidom::Element;
use xmpp_parsers::minidom::rxml::NcName;
use xmpp_parsers::ns;
use xmpp_parsers::pubsub::event::{Event, Payload};
use xmpp_parsers::stanza_error::{DefinedCondition, StanzaError};

use super::{
    Ctx, IqResponse, Pending as FeaturePending, PendingIq, avatars, bookmarks, mds, pins, spaces,
    tune,
};

pub const NODE_BOOKMARKS: &str = "urn:xmpp:bookmarks:1";
pub const NODE_AVATAR_METADATA: &str = "urn:xmpp:avatar:metadata";
pub const NODE_MDS: &str = mds::NODE_MDS;

/// A message that carries a pubsub event (also PEP). Returns true if it is one.
pub(crate) fn on_event(ctx: &mut Ctx<'_>, message: &Message) -> bool {
    // A join request for a space that we own is a data form, not an event.
    if spaces::on_authorization(ctx, message) {
        return true;
    }
    let Some(event) = message
        .payloads
        .iter()
        .find_map(|p| Event::try_from(p.clone()).ok())
    else {
        return false;
    };
    // PEP events come from the bare JID of the account that owns the node. A missing
    // `from` means our own account.
    let from = message
        .from
        .clone()
        .unwrap_or_else(|| Jid::from(ctx.account.clone()));
    match node_of(&event.payload) {
        NODE_BOOKMARKS => {
            if from.to_bare() == *ctx.account {
                bookmarks::on_event(ctx, event.payload);
            } else {
                log::warn!("dropped a bookmarks event from {from}: not our account");
            }
        }
        pins::NODE_PINS => {
            if from.to_bare() == *ctx.account {
                pins::on_event(ctx, event.payload);
            } else {
                log::warn!("dropped a pins event from {from}: not our account");
            }
        }
        NODE_MDS => {
            if from.to_bare() == *ctx.account {
                mds::on_event(ctx, event.payload);
            } else {
                log::warn!("dropped an MDS event from {from}: not our account");
            }
        }
        NODE_AVATAR_METADATA => avatars::on_metadata_event(ctx, &from.to_bare(), event.payload),
        tune::NODE_TUNE => tune::on_event(ctx, &from.to_bare(), event.payload),
        _ => spaces::on_event(ctx, &from, event.payload),
    }
    true
}

/// The node name of an event payload.
pub fn node_of(payload: &Payload) -> &str {
    match payload {
        Payload::Configuration { node, .. }
        | Payload::Delete { node, .. }
        | Payload::Items { node, .. }
        | Payload::Purge { node }
        | Payload::Subscription { node, .. } => &node.0,
    }
}

/// The node and the options of a publish, kept to send it again after a `conflict`.
#[derive(Debug)]
pub(crate) struct Retry {
    iq: Iq,
    node: String,
    wanted: DataForm,
}

impl Retry {
    /// `None` if `iq` is no publish with options.
    pub(crate) fn of(iq: &Iq) -> Option<Self> {
        let Iq::Set { payload, .. } = iq else {
            return None;
        };
        if !payload.is("pubsub", ns::PUBSUB) {
            return None;
        }
        let node = payload
            .get_child("publish", ns::PUBSUB)?
            .attr("node")?
            .to_owned();
        let form = payload
            .get_child("publish-options", ns::PUBSUB)?
            .get_child("x", ns::DATA_FORMS)?;
        let wanted = DataForm::try_from(form.clone()).ok()?;
        Some(Self {
            iq: iq.clone(),
            node,
            wanted,
        })
    }
}

/// A publish that waits for the node configuration steps.
#[derive(Debug)]
pub(crate) struct Held {
    retry: Retry,
    /// What the feature wanted to do with the answer to the publish.
    then: FeaturePending,
    /// The answer to the first publish. The feature gets it if a step fails.
    error: StanzaError,
}

/// What to do with the answer to an IQ of the configuration steps.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The node configuration form.
    Fetch(Box<Held>),
    /// The answer to our configuration submit.
    Submit(Box<Held>),
}

/// True for the answer of a node that has another configuration than the publish options.
fn is_conflict(error: &StanzaError) -> bool {
    error.defined_condition == DefinedCondition::Conflict
}

/// The answer to an IQ, with its request. A publish with options that the service refuses
/// with `conflict` starts the configuration steps. Any other answer goes to the feature.
pub(crate) fn on_answer(ctx: &mut Ctx<'_>, pending: PendingIq, response: IqResponse) {
    let PendingIq {
        to, then, retry, ..
    } = pending;
    if let Some(retry) = retry
        && let IqResponse::Error(error) = &response
        && is_conflict(error)
    {
        let iq = Iq::Get {
            from: None,
            to,
            id: String::new(),
            payload: owner_configure(&retry.node, None),
        };
        let held = Held {
            retry: *retry,
            then,
            error: error.clone(),
        };
        log::info!(
            "publish to {} refused with conflict. Reading the node configuration.",
            held.retry.node
        );
        ctx.request(iq, FeaturePending::Pubsub(Pending::Fetch(Box::new(held))));
        return;
    }
    super::on_iq_response(ctx, then, response);
}

/// An owner `<configure/>` request for `node`, with the form to submit if there is one.
fn owner_configure(node: &str, form: Option<DataForm>) -> Element {
    let mut configure = Element::builder("configure", ns::PUBSUB_OWNER).attr(
        NcName::try_from("node").expect("a valid attribute name"),
        node,
    );
    if let Some(form) = form {
        configure = configure.append(Element::from(form));
    }
    Element::builder("pubsub", ns::PUBSUB_OWNER)
        .append(configure)
        .build()
}

/// The submit that sets the wanted values: each wanted field that the node form offers
/// and that has another value now. `None` if nothing differs.
fn config_submit(current: &DataForm, wanted: &DataForm) -> Option<DataForm> {
    let fields: Vec<_> = wanted
        .fields
        .iter()
        .filter(|w| w.var.as_deref().is_some_and(|v| v != "FORM_TYPE"))
        .filter(|w| {
            current
                .fields
                .iter()
                .find(|c| c.var == w.var)
                .is_some_and(|c| c.values != w.values)
        })
        .cloned()
        .collect();
    if fields.is_empty() {
        return None;
    }
    Some(DataForm::new(
        DataFormType::Submit,
        ns::PUBSUB_CONFIGURE,
        fields,
    ))
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Fetch(held) => {
            let current = match &response {
                IqResponse::Result(Some(payload)) => payload
                    .get_child("configure", ns::PUBSUB_OWNER)
                    .and_then(|c| c.get_child("x", ns::DATA_FORMS))
                    .and_then(|x| DataForm::try_from(x.clone()).ok()),
                _ => None,
            };
            let Some(form) = current.and_then(|c| config_submit(&c, &held.retry.wanted)) else {
                return give_up(ctx, *held);
            };
            let iq = Iq::Set {
                from: None,
                to: held.retry.iq.to().cloned(),
                id: String::new(),
                payload: owner_configure(&held.retry.node, Some(form)),
            };
            ctx.request(iq, FeaturePending::Pubsub(Pending::Submit(held)));
        }
        Pending::Submit(held) => match response {
            IqResponse::Result(_) => {
                // The second publish has no retry: one try only.
                let Held { retry, then, .. } = *held;
                ctx.request(retry.iq, then);
            }
            _ => give_up(ctx, *held),
        },
    }
}

/// A step failed. The feature gets the answer to the first publish.
fn give_up(ctx: &mut Ctx<'_>, held: Held) {
    super::on_iq_response(ctx, held.then, IqResponse::Error(held.error));
}

#[cfg(test)]
mod tests {
    use futures_channel::oneshot;
    use jid::BareJid;
    use xmpp_parsers::stanza_error::ErrorType;

    use super::*;
    use crate::actor::ClientError;
    use crate::features::testing::Harness;

    const ROOM: &str = "room@rooms.chord.localhost";

    fn conflict() -> IqResponse {
        IqResponse::Error(StanzaError::new(
            ErrorType::Cancel,
            DefinedCondition::Conflict,
            "en",
            "precondition-not-met",
        ))
    }

    fn forbidden() -> IqResponse {
        IqResponse::Error(StanzaError::new(
            ErrorType::Auth,
            DefinedCondition::Forbidden,
            "en",
            "",
        ))
    }

    fn is_bookmarks(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Bookmarks(_))
    }

    fn is_fetch(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Pubsub(Pending::Fetch(_)))
    }

    fn is_submit(p: &FeaturePending) -> bool {
        matches!(p, FeaturePending::Pubsub(Pending::Submit(_)))
    }

    /// The form that a node with another configuration sends.
    fn node_form(access: &str) -> Element {
        format!(
            "<pubsub xmlns='{}'><configure node='urn:xmpp:bookmarks:1'>
               <x xmlns='jabber:x:data' type='form'>
                 <field var='FORM_TYPE' type='hidden'><value>{}</value></field>
                 <field var='pubsub#persist_items' type='boolean'><value>true</value></field>
                 <field var='pubsub#max_items'><value>10</value></field>
                 <field var='pubsub#access_model' type='list-single'><value>{access}</value></field>
               </x></configure></pubsub>",
            ns::PUBSUB_OWNER,
            ns::PUBSUB_CONFIGURE
        )
        .parse()
        .unwrap()
    }

    fn publish(h: &mut Harness) -> oneshot::Receiver<Result<(), ClientError>> {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            bookmarks::add(
                ctx,
                BareJid::new(ROOM).unwrap(),
                Some("Room".into()),
                true,
                Some("al".into()),
                None,
                reply,
            )
        });
        answer
    }

    #[test]
    fn conflict_reads_the_config_submits_it_and_publishes_again() {
        let mut h = Harness::new();
        let mut answer = publish(&mut h);
        assert_eq!(h.sent_iqs().len(), 1);

        // The feature does not see the conflict. The client asks for the node config.
        h.respond(is_bookmarks, conflict());
        assert!(answer.try_recv().unwrap().is_none());
        let iqs = h.sent_iqs();
        let Iq::Get { payload, to, .. } = &iqs[0] else {
            panic!("not a get")
        };
        assert!(to.is_none(), "PEP: our own account");
        let configure = payload.get_child("configure", ns::PUBSUB_OWNER).unwrap();
        assert_eq!(configure.attr("node"), Some("urn:xmpp:bookmarks:1"));

        // It submits the wanted values that differ, and only those.
        h.answer(is_fetch, Some(node_form("presence")));
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set")
        };
        let x = payload
            .get_child("configure", ns::PUBSUB_OWNER)
            .unwrap()
            .get_child("x", ns::DATA_FORMS)
            .unwrap();
        let form = DataForm::try_from(x.clone()).unwrap();
        assert_eq!(form.type_, DataFormType::Submit);
        assert_eq!(form.form_type(), Some(ns::PUBSUB_CONFIGURE));
        let vars: Vec<_> = form
            .fields
            .iter()
            .filter_map(|f| f.var.as_deref())
            .collect();
        assert_eq!(
            vars,
            ["FORM_TYPE", "pubsub#max_items", "pubsub#access_model"]
        );

        // The publish goes out again, and its answer reaches the feature.
        h.answer(is_submit, None);
        let iqs = h.sent_iqs();
        let Iq::Set { payload, .. } = &iqs[0] else {
            panic!("not a set")
        };
        assert!(payload.get_child("publish", ns::PUBSUB).is_some());
        assert!(answer.try_recv().unwrap().is_none());
        h.answer(is_bookmarks, None);
        assert_eq!(answer.try_recv().unwrap(), Some(Ok(())));
    }

    #[test]
    fn a_second_conflict_reaches_the_feature() {
        let mut h = Harness::new();
        let mut answer = publish(&mut h);
        h.respond(is_bookmarks, conflict());
        h.answer(is_fetch, Some(node_form("presence")));
        h.answer(is_submit, None);
        h.sent_iqs();
        // One retry only.
        h.respond(is_bookmarks, conflict());
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
        assert!(h.sent_iqs().is_empty());
    }

    #[test]
    fn a_refused_config_gives_the_first_error_to_the_feature() {
        let mut h = Harness::new();
        let mut answer = publish(&mut h);
        h.respond(is_bookmarks, conflict());
        h.answer(is_fetch, Some(node_form("presence")));
        h.respond(is_submit, forbidden());
        let Some(Err(ClientError::Server(text))) = answer.try_recv().unwrap() else {
            panic!("no error")
        };
        assert!(text.contains("precondition-not-met"), "{text}");
    }

    #[test]
    fn a_node_that_has_the_options_already_gives_the_error_back() {
        let mut h = Harness::new();
        let mut answer = publish(&mut h);
        h.respond(is_bookmarks, conflict());
        let same = format!(
            "<pubsub xmlns='{}'><configure node='urn:xmpp:bookmarks:1'>
               <x xmlns='jabber:x:data' type='form'>
                 <field var='pubsub#access_model'><value>whitelist</value></field>
                 <field var='pubsub#max_items'><value>max</value></field>
               </x></configure></pubsub>",
            ns::PUBSUB_OWNER
        );
        h.answer(is_fetch, Some(same.parse().unwrap()));
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
    }

    #[test]
    fn another_error_is_no_conflict() {
        let mut h = Harness::new();
        let mut answer = publish(&mut h);
        h.respond(is_bookmarks, forbidden());
        assert!(matches!(
            answer.try_recv().unwrap(),
            Some(Err(ClientError::Server(_)))
        ));
        assert_eq!(h.sent_iqs().len(), 1, "only the publish");
    }
}
