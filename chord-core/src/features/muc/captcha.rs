//! The CAPTCHA of a room join (XEP-0158). A room that wants proof of a human holds our
//! join and sends a message with a `<captcha/>` child: a data form with the challenge, and
//! the image in a `<data/>` element (XEP-0231). The answer is an IQ set to the room. The
//! room lets us in when the answer is right, so the join that waits stays as it is.
//!
//! The form is the model of `forms.rs`, the one that the registration CAPTCHA (XEP-0077)
//! and the ad-hoc forms use, so a UI shows it with the same form renderer.

use jid::{BareJid, Jid};
use xmpp_parsers::iq::Iq;
use xmpp_parsers::minidom::Element;

use super::{Command, Ctx, IqResponse, Pending, Reply, error_text};
use crate::actor::{ClientError, ClientEvent, ClientHandle};
use crate::forms::Form;

const NS_CAPTCHA: &str = "urn:xmpp:captcha";

impl ClientHandle {
    /// Answer the CAPTCHA of a room that a `ClientEvent::RoomCaptcha` reported. `form` is
    /// the form of the event with the answer in its values. The room lets us in when the
    /// answer is right. A wrong answer fails with `not-acceptable`, and the room can send a
    /// new challenge.
    pub async fn answer_room_captcha(&self, room: BareJid, form: Form) -> Result<(), ClientError> {
        let problems = form.problems();
        if !problems.is_empty() {
            return Err(ClientError::Invalid(problems.join(", ")));
        }
        self.room_command(|reply| Command::Captcha {
            room,
            answer: Some(form),
            reply,
        })
        .await
    }

    /// Give up the CAPTCHA of a room: tell the room, and stop the join.
    pub async fn cancel_room_captcha(&self, room: BareJid) -> Result<(), ClientError> {
        self.room_command(|reply| Command::Captcha {
            room,
            answer: None,
            reply,
        })
        .await
    }
}

/// The challenge form in the payloads of a message or a presence. The image comes as a
/// `data:` URI when the stanza carries it.
pub(super) fn challenge(payloads: &[Element]) -> Option<Form> {
    let captcha = payloads.iter().find(|p| p.is("captcha", NS_CAPTCHA))?;
    let x = captcha
        .children()
        .find(|c| c.is("x", crate::forms::NS_DATA))?;
    let mut form = Form::from_element(x).ok()?;
    // The image is a sibling of the captcha element (XEP-0158, 3.1).
    let mut parent = Element::builder("message", "jabber:client").build();
    for payload in payloads {
        parent.append_child(payload.clone());
    }
    form.resolve_bob(&parent);
    Some(form)
}

/// A stanza from a room that holds our join: is there a CAPTCHA in it? If so, tell the
/// frontends. Only a room that we are joining can ask. Returns true when it did.
pub(super) fn on_challenge(ctx: &mut Ctx<'_>, room: &BareJid, payloads: &[Element]) -> bool {
    let Some(join) = ctx.state.muc.joins.get_mut(room) else {
        return false;
    };
    let Some(form) = challenge(payloads) else {
        return false;
    };
    // A person needs time: the join timeout starts again.
    join.ticks = 0;
    ctx.emit(ClientEvent::RoomCaptcha {
        room: room.clone(),
        form,
    });
    true
}

/// The answer of a person (`answer` is the filled form), or the cancel (`None`).
pub(super) fn on_command(ctx: &mut Ctx<'_>, room: BareJid, answer: Option<Form>, reply: Reply) {
    if !ctx.state.muc.joins.contains_key(&room) {
        let _ = reply.send(Err(ClientError::Invalid(format!(
            "{room} asks for no CAPTCHA now"
        ))));
        return;
    }
    let cancelled = answer.is_none();
    let x = answer.map_or_else(Form::cancellation, |form| form.submission());
    let iq = Iq::Set {
        from: None,
        to: Some(Jid::from(room.clone())),
        id: String::new(),
        payload: Element::builder("captcha", NS_CAPTCHA).append(x).build(),
    };
    if cancelled {
        ctx.send(iq);
        super::leave_room_quietly(ctx, &room);
        let _ = reply.send(Ok(()));
        return;
    }
    if let Some(join) = ctx.state.muc.joins.get_mut(&room) {
        join.ticks = 0;
    }
    ctx.request(iq, crate::features::Pending::Muc(Pending::Captcha(reply)));
}

/// The answer of the room to our CAPTCHA answer.
pub(super) fn on_answer(_ctx: &mut Ctx<'_>, reply: Reply, response: IqResponse) {
    let _ = reply.send(match response {
        IqResponse::Result(_) => Ok(()),
        IqResponse::Error(e) => Err(ClientError::Server(error_text(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    });
}
