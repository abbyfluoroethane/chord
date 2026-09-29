//! HTTP file upload (XEP-0363): slot request and upload.
//!
//! `ClientHandle::upload` asks the upload service for a slot, then the runtime runs the
//! HTTP PUT, then we send a chat message with the GET URL. It needs the disco scan of the
//! server to be complete: `features::on_command` holds the command until then.
//!
//! For a room, the URL goes out as a groupchat message (through the MUC outbox if the
//! join still runs).

use std::collections::HashMap;

use futures_channel::oneshot;
use jid::Jid;
use xmpp_parsers::disco::DiscoInfoResult;
use xmpp_parsers::iq::Iq;
use xmpp_parsers::oob::Oob;
use xmpp_parsers::stanza_error::StanzaError;

use super::{Ctx, FeatureCommand, IqResponse, Pending as FeaturePending, chat};
use crate::actor::{ClientError, ClientHandle};

const NS_UPLOAD: &str = "urn:xmpp:http:upload:0";

type Reply = oneshot::Sender<Result<String, ClientError>>;

/// In-memory state for one session.
#[derive(Debug, Default)]
pub(crate) struct State {
    next_id: u64,
    transfers: HashMap<u64, Transfer>,
}

/// One upload in progress.
#[derive(Debug)]
struct Transfer {
    to: Jid,
    content_type: String,
    /// The file, until the slot answer moves it to the `PutRequest`.
    data: Vec<u8>,
    /// Set when the slot answer arrives.
    get_url: Option<String>,
    reply: Reply,
}

/// What to do with the answer to an IQ that this feature sent.
#[derive(Debug)]
pub(crate) enum Pending {
    /// The slot request of the transfer with this id.
    Slot(u64),
}

/// A command from the public API.
pub(crate) enum Command {
    Upload {
        to: Jid,
        filename: String,
        content_type: String,
        data: Vec<u8>,
        reply: Reply,
    },
}

impl ClientHandle {
    /// Upload a file to the upload service of the server (XEP-0363) and send its GET URL
    /// to `to` as a chat message with an out-of-band link. Returns the GET URL.
    ///
    /// Fails with `Unsupported` when the server scan is not complete yet, when the server
    /// has no upload service, and for a room.
    pub async fn upload(
        &self,
        to: Jid,
        filename: String,
        content_type: String,
        data: Vec<u8>,
    ) -> Result<String, ClientError> {
        let (reply, answer) = oneshot::channel();
        self.feature(FeatureCommand::Upload(Command::Upload {
            to,
            filename,
            content_type,
            data,
            reply,
        }))?;
        answer.await.map_err(|_| ClientError::ActorGone)?
    }
}

pub(crate) fn on_response(ctx: &mut Ctx<'_>, pending: Pending, response: IqResponse) {
    match pending {
        Pending::Slot(id) => on_slot(ctx, id, response),
    }
}

pub(crate) fn on_command(ctx: &mut Ctx<'_>, command: Command) {
    match command {
        Command::Upload {
            to,
            filename,
            content_type,
            data,
            reply,
        } => {
            if let Err((e, reply)) = start_upload(ctx, to, filename, content_type, data, reply) {
                let _ = reply.send(Err(e));
            }
        }
    }
}

/// A command while no session is up. Answer each reply channel with an error.
pub(crate) fn offline(command: Command) {
    match command {
        Command::Upload { reply, .. } => {
            let _ = reply.send(Err(ClientError::NotConnected));
        }
    }
}

fn start_upload(
    ctx: &mut Ctx<'_>,
    to: Jid,
    filename: String,
    content_type: String,
    data: Vec<u8>,
    reply: Reply,
) -> Result<(), (ClientError, Reply)> {
    if filename.is_empty() {
        return Err((ClientError::Invalid("the file name is empty".into()), reply));
    }
    let (service, info) = match ctx.state.disco.find_feature(NS_UPLOAD) {
        Some((jid, info)) => (jid.clone(), info),
        None if ctx.state.disco.complete => {
            let e = ClientError::Unsupported("the server has no upload service".into());
            return Err((e, reply));
        }
        None => {
            let e = ClientError::Unsupported("upload service not discovered yet".into());
            return Err((e, reply));
        }
    };
    let size = data.len() as u64;
    if let Some(max) = max_file_size(info)
        && size > max
    {
        let e = ClientError::Invalid(format!(
            "the file has {size} bytes, and the service accepts {max} at most"
        ));
        return Err((e, reply));
    }

    let id = ctx.state.upload.next_id;
    ctx.state.upload.next_id += 1;
    let request = xmpp_parsers::http_upload::SlotRequest {
        filename,
        size,
        content_type: Some(content_type.clone()),
    };
    ctx.state.upload.transfers.insert(
        id,
        Transfer {
            to,
            content_type,
            data,
            get_url: None,
            reply,
        },
    );
    ctx.request(
        Iq::from_get("", request).with_to(service),
        FeaturePending::Upload(Pending::Slot(id)),
    );
    Ok(())
}

/// Whether `to` is a room that we know.
fn is_room(ctx: &Ctx<'_>, to: &Jid) -> bool {
    let found = ctx.store.conn().query_row(
        "SELECT 1 FROM rooms WHERE account_id = ?1 AND jid = ?2",
        rusqlite::params![ctx.account_id, to.to_bare().as_str()],
        |_| Ok(()),
    );
    found.is_ok()
}

/// The `max-file-size` of the upload service, in bytes.
fn max_file_size(info: &DiscoInfoResult) -> Option<u64> {
    info.extensions
        .iter()
        .find(|form| form.form_type() == Some(NS_UPLOAD))?
        .fields
        .iter()
        .find(|field| field.var.as_deref() == Some("max-file-size"))?
        .values
        .first()?
        .trim()
        .parse()
        .ok()
}

fn on_slot(ctx: &mut Ctx<'_>, id: u64, response: IqResponse) {
    let Some(mut transfer) = ctx.state.upload.transfers.remove(&id) else {
        return;
    };
    let slot = match response {
        IqResponse::Result(Some(payload)) => {
            xmpp_parsers::http_upload::SlotResult::try_from(payload)
                .map_err(|e| ClientError::Server(format!("bad upload slot: {e}")))
        }
        IqResponse::Result(None) => Err(ClientError::Server("empty upload slot".into())),
        IqResponse::Error(e) => Err(ClientError::Server(describe(&e))),
        IqResponse::Lost => Err(ClientError::NotConnected),
    };
    let slot = match slot {
        Ok(slot) => slot,
        Err(e) => {
            let _ = transfer.reply.send(Err(e));
            return;
        }
    };
    // The service is not trusted with the scheme (XEP-0363, section 6).
    for url in [&slot.put.url, &slot.get.url] {
        if let Err(e) = check_url(url) {
            let _ = transfer.reply.send(Err(ClientError::Server(e)));
            return;
        }
    }
    // The parser keeps only Authorization, Cookie, and Expires (XEP-0363, section 4).
    let headers = slot
        .put
        .headers
        .iter()
        .map(|h| (h.name.as_str().to_owned(), h.value.clone()))
        .collect();
    transfer.get_url = Some(slot.get.url);
    ctx.upload(PutRequest {
        id,
        url: slot.put.url,
        headers,
        content_type: transfer.content_type.clone(),
        data: std::mem::take(&mut transfer.data),
    });
    ctx.state.upload.transfers.insert(id, transfer);
}

fn describe(error: &StanzaError) -> String {
    let text = error.texts.values().next().map(String::as_str);
    match text {
        Some(text) => format!("{:?}: {text}", error.defined_condition),
        None => format!("{:?}", error.defined_condition),
    }
}

/// Accept an https URL, or an http URL to `localhost` or `127.0.0.1` (a dev server).
fn check_url(url: &str) -> Result<(), String> {
    let bad = || format!("the upload service gave an unsafe URL: {url}");
    let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // No user info: it can hide the real host.
    if authority.is_empty() || authority.contains('@') {
        return Err(bad());
    }
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    let local = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1";
    if scheme.eq_ignore_ascii_case("https") || (scheme.eq_ignore_ascii_case("http") && local) {
        Ok(())
    } else {
        Err(bad())
    }
}

/// An HTTP PUT for the runtime to run.
#[derive(Debug)]
pub(crate) struct PutRequest {
    pub id: u64,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// The result of a `PutRequest`.
#[derive(Debug)]
pub(crate) struct PutDone {
    pub id: u64,
    pub result: Result<(), String>,
}

/// Run a PUT. Send the result to `done` as `Internal::UploadDone`.
#[cfg(feature = "native-session")]
pub(crate) fn start(
    request: PutRequest,
    done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    crate::runtime::spawn(async move {
        let result = crate::runtime::http_put(
            &request.url,
            &request.headers,
            &request.content_type,
            request.data,
        )
        .await;
        let done_message = super::Internal::UploadDone(PutDone {
            id: request.id,
            result,
        });
        // An error means that the actor stopped. Nobody waits for the result.
        let _ = done.unbounded_send(done_message);
    });
}

/// Without the native session there is no HTTP client yet. Fail at once.
#[cfg(not(feature = "native-session"))]
pub(crate) fn start(
    request: PutRequest,
    done: futures_channel::mpsc::UnboundedSender<super::Internal>,
) {
    let _ = done.unbounded_send(super::Internal::UploadDone(PutDone {
        id: request.id,
        result: Err("HTTP upload is not available in this build".into()),
    }));
}

/// A PUT finished.
pub(crate) fn on_put_done(ctx: &mut Ctx<'_>, done: PutDone) {
    let Some(transfer) = ctx.state.upload.transfers.remove(&done.id) else {
        return;
    };
    let Some(get_url) = transfer.get_url else {
        return;
    };
    match done.result {
        Ok(()) => {
            let oob = Oob {
                url: get_url.clone(),
                desc: None,
            };
            if is_room(ctx, &transfer.to) {
                let room = transfer.to.to_bare();
                let sent =
                    super::muc::send_with_payload(ctx, &room, get_url.clone(), Some(oob.into()));
                let _ = transfer.reply.send(sent.map(|_| get_url));
            } else {
                chat::send_with_oob(ctx, transfer.to, get_url.clone(), Some(oob));
                let _ = transfer.reply.send(Ok(get_url));
            }
        }
        Err(e) => {
            let _ = transfer.reply.send(Err(ClientError::Server(e)));
        }
    }
}

#[cfg(test)]
mod tests {
    use xmpp_parsers::data_forms::{DataForm, DataFormType, Field, FieldType};
    use xmpp_parsers::message::Message;
    use xmpp_parsers::minidom::Element;
    use xmpp_parsers::stanza::Stanza;
    use xmpp_parsers::stanza_error::{DefinedCondition, ErrorType};

    use super::*;
    use crate::features::testing::Harness;
    use crate::features::{Effect, Internal, on_command, on_internal};

    type Answer = oneshot::Receiver<Result<String, ClientError>>;

    fn jid(s: &str) -> Jid {
        Jid::new(s).unwrap()
    }

    fn harness(max: Option<u64>) -> Harness {
        let mut h = Harness::new();
        let mut info = crate::features::disco::info(None);
        info.features.insert(NS_UPLOAD.into());
        if let Some(max) = max {
            let field =
                Field::new("max-file-size", FieldType::TextSingle).with_value(&max.to_string());
            info.extensions
                .push(DataForm::new(DataFormType::Result_, NS_UPLOAD, vec![field]));
        }
        h.state
            .disco
            .services
            .push((jid("upload.chord.localhost"), info));
        h.state.disco.complete = true;
        h
    }

    fn command(h: &mut Harness, to: &str, data: Vec<u8>) -> Answer {
        let (reply, answer) = oneshot::channel();
        h.with_ctx(|ctx| {
            on_command(
                ctx,
                FeatureCommand::Upload(Command::Upload {
                    to: jid(to),
                    filename: "cat.png".into(),
                    content_type: "image/png".into(),
                    data,
                    reply,
                }),
            )
        });
        answer
    }

    fn slot(put: &str, get: &str) -> Element {
        format!(
            "<slot xmlns='{NS_UPLOAD}'><put url='{put}'>\
             <header name='Authorization'>Basic abc</header>\
             <header name='Cookie'>a=b</header></put><get url='{get}'/></slot>"
        )
        .parse()
        .unwrap()
    }

    fn answer_slot(h: &mut Harness, response: IqResponse) {
        h.respond(|p| matches!(p, FeaturePending::Upload(_)), response);
    }

    fn take_put(h: &mut Harness) -> Option<PutRequest> {
        let mut found = None;
        let effects = std::mem::take(&mut h.effects);
        for effect in effects {
            match effect {
                Effect::Upload(request) => found = Some(request),
                other => h.effects.push(other),
            }
        }
        found
    }

    fn finish(h: &mut Harness, id: u64, result: Result<(), String>) {
        h.with_ctx(|ctx| on_internal(ctx, Internal::UploadDone(PutDone { id, result })));
    }

    fn result_of(mut answer: Answer) -> Result<String, ClientError> {
        answer.try_recv().unwrap().expect("no answer yet")
    }

    #[test]
    fn slot_request_has_name_size_and_type() {
        let mut h = harness(Some(1000));
        let mut answer = command(&mut h, "bob@chord.localhost", vec![1, 2, 3]);
        assert!(answer.try_recv().unwrap().is_none());
        let iqs = h.sent_iqs();
        let Iq::Get { to, payload, .. } = &iqs[0] else {
            panic!("not a get");
        };
        assert_eq!(to.as_ref().unwrap().as_str(), "upload.chord.localhost");
        assert!(payload.is("request", NS_UPLOAD));
        assert_eq!(payload.attr("filename"), Some("cat.png"));
        assert_eq!(payload.attr("size"), Some("3"));
        assert_eq!(payload.attr("content-type"), Some("image/png"));
    }

    #[test]
    fn success_puts_then_sends_the_message_with_oob() {
        let mut h = harness(None);
        let answer = command(&mut h, "bob@chord.localhost", vec![1, 2, 3]);
        h.take_sent();
        let payload = slot("https://up.example/put/1", "https://up.example/get/1");
        answer_slot(&mut h, IqResponse::Result(Some(payload)));

        let put = take_put(&mut h).expect("a PUT request");
        assert_eq!(put.url, "https://up.example/put/1");
        assert_eq!(put.content_type, "image/png");
        assert_eq!(put.data, vec![1, 2, 3]);
        assert_eq!(
            put.headers,
            vec![
                ("Authorization".to_owned(), "Basic abc".to_owned()),
                ("Cookie".to_owned(), "a=b".to_owned())
            ]
        );
        // The message waits for the PUT.
        assert!(h.take_sent().is_empty());

        finish(&mut h, put.id, Ok(()));
        assert_eq!(
            result_of(answer).unwrap(),
            "https://up.example/get/1".to_owned()
        );
        let sent = h.take_sent();
        let Stanza::Message(message): &Stanza = &sent[0] else {
            panic!("not a message");
        };
        let message: &Message = message;
        assert_eq!(message.to.as_ref().unwrap().as_str(), "bob@chord.localhost");
        let (_, body) = message.get_best_body(vec![]).unwrap();
        assert_eq!(body, "https://up.example/get/1");
        let oob = message
            .payloads
            .iter()
            .find_map(|p| Oob::try_from(p.clone()).ok())
            .expect("an OOB payload");
        assert_eq!(oob.url, "https://up.example/get/1");
    }

    #[test]
    fn localhost_http_is_allowed() {
        let mut h = harness(None);
        let _answer = command(&mut h, "bob@chord.localhost", vec![1]);
        let payload = slot("http://localhost:5280/put", "http://127.0.0.1:5280/get");
        answer_slot(&mut h, IqResponse::Result(Some(payload)));
        assert!(take_put(&mut h).is_some());
    }

    #[test]
    fn non_https_urls_are_refused() {
        for (put, get) in [
            ("http://up.example/put", "https://up.example/get"),
            ("https://up.example/put", "http://up.example/get"),
            (
                "http://localhost@evil.example/put",
                "https://up.example/get",
            ),
            (
                "http://localhost.evil.example/put",
                "https://up.example/get",
            ),
            ("ftp://localhost/put", "https://up.example/get"),
        ] {
            let mut h = harness(None);
            let answer = command(&mut h, "bob@chord.localhost", vec![1]);
            answer_slot(&mut h, IqResponse::Result(Some(slot(put, get))));
            assert!(take_put(&mut h).is_none(), "{put} {get}");
            assert!(
                matches!(result_of(answer), Err(ClientError::Server(_))),
                "{put} {get}"
            );
        }
    }

    #[test]
    fn slot_error_fails_the_command() {
        let mut h = harness(None);
        let answer = command(&mut h, "bob@chord.localhost", vec![1]);
        let error = StanzaError::new(
            ErrorType::Modify,
            DefinedCondition::NotAcceptable,
            "en",
            "too big",
        );
        answer_slot(&mut h, IqResponse::Error(error));
        let Err(ClientError::Server(text)) = result_of(answer) else {
            panic!("expected a server error");
        };
        assert!(text.contains("too big"), "{text}");
        assert!(take_put(&mut h).is_none());
    }

    #[test]
    fn bad_slot_and_lost_fail_the_command() {
        let mut h = harness(None);
        let answer = command(&mut h, "bob@chord.localhost", vec![1]);
        answer_slot(&mut h, IqResponse::Result(None));
        assert!(matches!(result_of(answer), Err(ClientError::Server(_))));

        let answer = command(&mut h, "bob@chord.localhost", vec![1]);
        answer_slot(&mut h, IqResponse::Lost);
        assert_eq!(result_of(answer), Err(ClientError::NotConnected));
        assert!(h.state.upload.transfers.is_empty());
    }

    #[test]
    fn too_large_file_is_refused_before_the_slot_request() {
        let mut h = harness(Some(2));
        let answer = command(&mut h, "bob@chord.localhost", vec![1, 2, 3]);
        assert!(matches!(result_of(answer), Err(ClientError::Invalid(_))));
        assert!(h.take_sent().is_empty());
        // A file at the limit passes.
        let mut answer = command(&mut h, "bob@chord.localhost", vec![1, 2]);
        assert!(answer.try_recv().unwrap().is_none());
        assert_eq!(h.sent_iqs().len(), 1);
    }

    #[test]
    fn put_failure_fails_the_command_and_sends_nothing() {
        let mut h = harness(None);
        let answer = command(&mut h, "bob@chord.localhost", vec![1]);
        h.take_sent();
        let payload = slot("https://up.example/put", "https://up.example/get");
        answer_slot(&mut h, IqResponse::Result(Some(payload)));
        let put = take_put(&mut h).unwrap();
        finish(&mut h, put.id, Err("the upload server answered 500".into()));
        assert!(matches!(result_of(answer), Err(ClientError::Server(_))));
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn no_service_fails_the_command() {
        let mut h = Harness::new();
        // Before discovery finishes, the command waits.
        let mut answer = command(&mut h, "bob@chord.localhost", vec![1]);
        assert_eq!(answer.try_recv().unwrap(), None);

        // Discovery finishes with no upload service: the command fails.
        h.state.disco.complete = true;
        h.with_ctx(crate::features::on_services_ready);
        let Err(ClientError::Unsupported(text)) = result_of(answer) else {
            panic!("expected Unsupported");
        };
        assert!(text.contains("no upload service"));
        assert!(h.take_sent().is_empty());
    }

    #[test]
    fn a_room_gets_a_groupchat_message_with_the_url() {
        let mut h = harness(None);
        let room = jid::BareJid::new("room@rooms.chord.localhost").unwrap();
        h.store
            .conn()
            .execute(
                "INSERT INTO rooms (account_id, jid, joined) VALUES (?1, ?2, 1)",
                rusqlite::params![h.account_id, room.as_str()],
            )
            .unwrap();
        h.state.muc.nicks.insert(room.clone(), "alice".into());
        let answer = command(&mut h, room.as_str(), vec![1]);
        let payload = slot("https://up.example/put", "https://up.example/get");
        answer_slot(&mut h, IqResponse::Result(Some(payload)));
        let put = take_put(&mut h).unwrap();
        finish(&mut h, put.id, Ok(()));
        assert_eq!(result_of(answer).unwrap(), "https://up.example/get");
        let sent = h.take_sent();
        let Some(Stanza::Message(m)) = sent.iter().find(|s| matches!(s, Stanza::Message(_))) else {
            panic!("{sent:?}")
        };
        assert_eq!(m.type_, xmpp_parsers::message::MessageType::Groupchat);
        assert!(m.payloads.iter().any(|p| Oob::try_from(p.clone()).is_ok()));
    }

    #[test]
    fn offline_answers_not_connected() {
        let (reply, mut answer) = oneshot::channel();
        offline(Command::Upload {
            to: jid("bob@chord.localhost"),
            filename: "a".into(),
            content_type: "text/plain".into(),
            data: vec![],
            reply,
        });
        assert_eq!(
            answer.try_recv().unwrap().unwrap(),
            Err(ClientError::NotConnected)
        );
    }

    #[test]
    fn url_check() {
        assert!(check_url("https://up.example/a").is_ok());
        assert!(check_url("HTTPS://up.example:8443/a").is_ok());
        assert!(check_url("http://localhost:5280/a").is_ok());
        assert!(check_url("http://127.0.0.1/a").is_ok());
        assert!(check_url("http://up.example/a").is_err());
        assert!(check_url("https:///a").is_err());
        assert!(check_url("no scheme").is_err());
    }
}
