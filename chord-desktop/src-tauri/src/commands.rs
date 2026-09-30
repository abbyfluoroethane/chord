//! The commands of the UI. Each one calls one `ClientHandle` method.
//!
//! JIDs arrive as strings and are parsed here. Errors leave as `ChordError`. The
//! TypeScript wrappers are in src/lib/chord/api.ts, with the same names in camelCase.

use std::path::PathBuf;

use chord_core::actor;
use chord_core::actor::ClientEvent;
use chord_core::features::muc::{RoomAffiliation, RoomCard, RoomRole, RoomSettings};
use chord_core::features::notify::{NotificationLevel, NotificationSetting};
use chord_core::features::profile::Profile;
use chord_core::features::push::PushRegistration;
use chord_core::features::roster::Contact;
use chord_core::features::spaces::{
    JoinOutcome, JoinRequest, PendingJoin, SpaceAccess, SpaceCard, SpaceInfo,
};
use chord_core::jid::{BareJid, Jid};
use chord_core::session::native::NativeSession;
use chord_core::session::{ServerAddr, SessionConfig};
use chord_core::store::{Store, queries};
use chord_core::views::{ChannelItem, ChannelScope, ListDiff, MemberItem, SpaceItem, TimelineItem};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::error::{ChordError, Res};
use crate::keychain;
use crate::notify;
use crate::state::{AppState, Client, TimelineStream};

// ---------------------------------------------------------------- parsing

fn bare(s: &str) -> Res<BareJid> {
    s.parse()
        .map_err(|e| ChordError::invalid(format!("not a bare JID ({s:?}): {e}")))
}

fn full(s: &str) -> Res<Jid> {
    s.parse()
        .map_err(|e| ChordError::invalid(format!("not a JID ({s:?}): {e}")))
}

/// The server field of `login`: none or empty for a SRV lookup, `starttls://host:port`, or
/// `xmpps://host:port` for direct TLS (XEP-0368).
pub fn parse_server(server: Option<&str>) -> Res<ServerAddr> {
    let bad = |s: &str| {
        ChordError::invalid(format!(
            "the server must be empty, starttls://host:port or xmpps://host:port, not {s:?}"
        ))
    };
    let s = match server.map(str::trim) {
        None | Some("") | Some("srv") => return Ok(ServerAddr::Srv),
        Some(s) => s,
    };
    let (direct_tls, rest) = match (s.strip_prefix("starttls://"), s.strip_prefix("xmpps://")) {
        (Some(rest), _) => (false, rest),
        (_, Some(rest)) => (true, rest),
        _ => return Err(bad(s)),
    };
    let (host, port) = rest.rsplit_once(':').ok_or_else(|| bad(s))?;
    let port: u16 = port.parse().map_err(|_| bad(s))?;
    if host.is_empty() || host.contains(['/', '@', ' ']) || port == 0 {
        return Err(bad(s));
    }
    let host = host.to_owned();
    Ok(if direct_tls {
        ServerAddr::DirectTls { host, port }
    } else {
        ServerAddr::StartTls { host, port }
    })
}

/// The text that the keychain keeps as the server of a saved password: `srv` or
/// `starttls://host:port` in lower case. It is the same text as the `server` of `login`.
pub fn server_key(server: Option<&str>) -> String {
    match server.map(str::trim) {
        None | Some("") => "srv".to_owned(),
        Some(s) => s.to_ascii_lowercase(),
    }
}

/// The password and the server text for a login. A password from the UI goes with the
/// server from the UI. With none, the saved password goes with the server it was saved
/// for, and the `server` argument is ignored. `saved` reads the keychain: the password and
/// its server. A saved password with no server entry is an old one and is for SRV.
pub fn credentials(
    password: Option<String>,
    server: Option<&str>,
    saved: impl FnOnce() -> Res<(Option<String>, Option<String>)>,
) -> Res<(String, String)> {
    if let Some(p) = password.filter(|p| !p.is_empty()) {
        return Ok((p, server_key(server)));
    }
    let (password, saved_server) = saved()?;
    let password =
        password.ok_or_else(|| ChordError::invalid("no password given and none saved"))?;
    Ok((password, server_key(saved_server.as_deref())))
}

/// The file name of an upload, from its path.
pub fn file_name(path: &std::path::Path) -> Res<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .ok_or_else(|| ChordError::invalid("the path has no file name"))
}

/// The media type of a file, from its extension. The UI can pass its own.
pub fn guess_content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// The biggest file that `upload` reads into memory: 100 MB. The upload service can
/// have a lower limit and answers with its own error.
pub const MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;

// ---------------------------------------------------------------- account

/// What `open` returns.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenInfo {
    /// The bare JID of the account.
    pub account: String,
    /// True if the keychain holds a password for it.
    pub has_saved_password: bool,
}

/// Open the account: start the client on its database. It works offline. The views and
/// the stored messages are there before `login`. A second call replaces the client.
#[tauri::command]
pub async fn open(app: AppHandle, state: State<'_, AppState>, account: String) -> Res<OpenInfo> {
    let jid = bare(&account)?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| ChordError::io("no data dir", e))?;
    let path: PathBuf = dir.join(format!("{jid}.sqlite3"));
    let jid2 = jid.clone();
    let (store, avatar_store, account_id) = tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)
            .map_err(|e| ChordError::io("cannot create the data dir", e))?;
        let store = Store::open(&path)?;
        let avatar_store = Store::open(&path)?;
        // A second connection waits for the actor instead of failing on a lock.
        avatar_store
            .conn()
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| ChordError::io("cannot set the busy timeout", e))?;
        let account_id = queries::ensure_account(avatar_store.conn(), jid2.as_str())
            .map_err(|e| ChordError::io("cannot read the account", e))?;
        Ok::<_, ChordError>((store, avatar_store, account_id))
    })
    .await
    .map_err(|e| ChordError::io("the open task failed", e))??;

    let (handle, events, actor) = actor::new::<NativeSession>(store, jid.clone())?;
    drop(tauri::async_runtime::spawn(actor.run()));
    let events_task =
        tauri::async_runtime::spawn(notify::pump(app.clone(), events, state.events.clone()));
    let client = Client::new(handle, jid.clone(), (avatar_store, account_id), events_task);
    // Drop the old client after the swap.
    drop(state.replace(client));
    Ok(OpenInfo {
        account: jid.to_string(),
        has_saved_password: keychain::get(jid.as_str()).ok().flatten().is_some(),
    })
}

/// Log in. `password` is optional: with none, the saved password is used, and it goes only
/// to the account and the server that it was saved for. Then `server` is ignored, because
/// a script in the page could use it to send the saved password to another host
/// (BRIDGESECURITY-08). Otherwise `server` is empty for a SRV lookup, or
/// `starttls://host:port`. With `remember`, a good login saves the password and the server
/// in the system keychain.
#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    password: Option<String>,
    server: Option<String>,
    remember: Option<bool>,
) -> Res<()> {
    let client = state.client()?;
    let (password, server_text) = credentials(password, server.as_deref(), || {
        let account = client.account.as_str();
        Ok((keychain::get(account)?, keychain::get_server(account)?))
    })?;
    let server = parse_server(Some(&server_text))?;
    let config = SessionConfig::new(client.account.clone(), password.clone(), server);
    client.handle.login(config).await?;
    if remember.unwrap_or(false) {
        keychain::set(client.account.as_str(), &password)?;
        keychain::set_server(client.account.as_str(), &server_text)?;
    }
    Ok(())
}

/// Log out. The server gets every queued stanza first. The client stays open.
#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Res<()> {
    state.handle()?.logout().await;
    Ok(())
}

/// True if the keychain holds a password for `account`.
#[tauri::command]
pub async fn saved_password(account: String) -> Res<bool> {
    Ok(keychain::get(bare(&account)?.as_str())?.is_some())
}

/// Delete the saved password of `account`.
#[tauri::command]
pub async fn forget_password(account: String) -> Res<()> {
    keychain::delete(bare(&account)?.as_str())
}

// ---------------------------------------------------------------- views

/// Subscribe to the spaces. The first message is a Reset. Returns the subscription id.
#[tauri::command]
pub async fn subscribe_space_list(
    state: State<'_, AppState>,
    on_diff: Channel<ListDiff<SpaceItem>>,
) -> Res<u64> {
    let client = state.client()?;
    let stream = client.handle.space_list().await?;
    Ok(client.subscribe(stream, on_diff, None))
}

/// Subscribe to the channels of Home or of one space.
#[tauri::command]
pub async fn subscribe_channel_list(
    state: State<'_, AppState>,
    scope: ChannelScope,
    on_diff: Channel<ListDiff<ChannelItem>>,
) -> Res<u64> {
    let client = state.client()?;
    let stream = client.handle.channel_list(scope).await?;
    Ok(client.subscribe(stream, on_diff, None))
}

/// Subscribe to the messages of a room or of a chat. `paginate_back` needs the id.
#[tauri::command]
pub async fn subscribe_timeline(
    state: State<'_, AppState>,
    peer: String,
    on_diff: Channel<ListDiff<TimelineItem>>,
) -> Res<u64> {
    let client = state.client()?;
    let timeline = client.handle.timeline(bare(&peer)?).await?;
    Ok(subscribe_timeline_of(&client, timeline, on_diff))
}

/// Subscribe to the private messages with one occupant of a room.
#[tauri::command]
pub async fn subscribe_private_timeline(
    state: State<'_, AppState>,
    room: String,
    nick: String,
    on_diff: Channel<ListDiff<TimelineItem>>,
) -> Res<u64> {
    let client = state.client()?;
    let timeline = client.handle.private_timeline(bare(&room)?, nick).await?;
    Ok(subscribe_timeline_of(&client, timeline, on_diff))
}

fn subscribe_timeline_of(
    client: &Client,
    timeline: actor::Timeline,
    on_diff: Channel<ListDiff<TimelineItem>>,
) -> u64 {
    let shared = std::sync::Arc::new(std::sync::Mutex::new(timeline));
    client.subscribe(TimelineStream(shared.clone()), on_diff, Some(shared))
}

/// Subscribe to the members of a room, or the two people of a chat.
#[tauri::command]
pub async fn subscribe_member_list(
    state: State<'_, AppState>,
    room: String,
    on_diff: Channel<ListDiff<MemberItem>>,
) -> Res<u64> {
    let client = state.client()?;
    let stream = client.handle.member_list(bare(&room)?).await?;
    Ok(client.subscribe(stream, on_diff, None))
}

/// Stop a subscription of any kind.
#[tauri::command]
pub async fn unsubscribe(state: State<'_, AppState>, id: u64) -> Res<()> {
    state.client()?.unsubscribe(id);
    Ok(())
}

/// Show `count` older messages in a timeline. If the store has fewer, MAM fetches them.
#[tauri::command]
pub async fn timeline_paginate_back(state: State<'_, AppState>, id: u64, count: usize) -> Res<()> {
    let timeline = state.client()?.timeline(id)?;
    let result = crate::state::lock(&timeline).paginate_back(count);
    Ok(result?)
}

/// Set the channel for the client events: connection state, notifications, typing,
/// invites, and notices. Events from before the call are kept (up to 200) and sent first.
#[tauri::command]
pub async fn events(state: State<'_, AppState>, on_event: Channel<ClientEvent>) -> Res<()> {
    crate::state::lock(&state.events).attach(on_event);
    Ok(())
}

// ---------------------------------------------------------------- messages

/// Send a chat message. Returns its origin-id.
#[tauri::command]
pub async fn send_chat(state: State<'_, AppState>, to: String, body: String) -> Res<String> {
    Ok(state.handle()?.send_chat(full(&to)?, body).await?)
}

/// Send a link that other clients show inline (XEP-0066), for example a GIF. Only an
/// https URL. Returns the origin-id.
#[tauri::command]
pub async fn send_link(state: State<'_, AppState>, to: String, url: String) -> Res<String> {
    let parsed = url::Url::parse(url.trim())
        .map_err(|e| ChordError::invalid(format!("not a URL ({url:?}): {e}")))?;
    if parsed.scheme() != "https" {
        return Err(ChordError::invalid(
            "only an https link can be sent as an embed",
        ));
    }
    Ok(state
        .handle()?
        .send_link(full(&to)?, parsed.to_string())
        .await?)
}

#[tauri::command]
pub async fn edit_message(state: State<'_, AppState>, item_id: String, body: String) -> Res<()> {
    Ok(state.handle()?.edit_message(item_id, body).await?)
}

#[tauri::command]
pub async fn retract_message(state: State<'_, AppState>, item_id: String) -> Res<()> {
    Ok(state.handle()?.retract_message(item_id).await?)
}

/// Delete the message of another user, as a room moderator (XEP-0425).
#[tauri::command]
pub async fn moderate_message(
    state: State<'_, AppState>,
    item_id: String,
    reason: Option<String>,
) -> Res<()> {
    Ok(state.handle()?.moderate_message(item_id, reason).await?)
}

#[tauri::command]
pub async fn reply(state: State<'_, AppState>, item_id: String, body: String) -> Res<()> {
    Ok(state.handle()?.reply(item_id, body).await?)
}

/// Replace our reactions to a message with `emojis`.
#[tauri::command]
pub async fn react(state: State<'_, AppState>, item_id: String, emojis: Vec<String>) -> Res<()> {
    Ok(state.handle()?.react(item_id, emojis).await?)
}

#[tauri::command]
pub async fn toggle_reaction(
    state: State<'_, AppState>,
    item_id: String,
    emoji: String,
) -> Res<()> {
    Ok(state.handle()?.toggle_reaction(item_id, emoji).await?)
}

/// Mark a chat or a room as read up to its newest message.
#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, peer: String) -> Res<()> {
    Ok(state.handle()?.mark_read(bare(&peer)?).await?)
}

/// Mark a message and the ones after it as unread. `item_id` is a timeline id (`m:<row>`).
/// The read position moves back. The command sends nothing to the server.
#[tauri::command]
pub async fn mark_unread(state: State<'_, AppState>, item_id: String) -> Res<()> {
    Ok(state.handle()?.mark_unread(item_id).await?)
}

#[tauri::command]
pub async fn mark_read_private(state: State<'_, AppState>, room: String, nick: String) -> Res<()> {
    Ok(state
        .handle()?
        .mark_read_private(bare(&room)?, nick)
        .await?)
}

/// Tell `peer` that we type or stopped (XEP-0085). See `ClientEvent::Typing` for `peer`.
#[tauri::command]
pub async fn set_typing(state: State<'_, AppState>, peer: String, typing: bool) -> Res<()> {
    Ok(state.handle()?.set_typing(peer, typing)?)
}

/// Tell the server that the window is in use (`true`) or not (`false`), with Client State
/// Indication (XEP-0352). Returns whether the server got it. Chord sends it again after a
/// reconnect, and keeps it while offline.
#[tauri::command]
pub async fn set_client_active(state: State<'_, AppState>, active: bool) -> Res<bool> {
    Ok(state.handle()?.set_client_active(active).await?)
}

/// Search the stored messages, newest first. With `peer`, only that chat or room.
/// Works offline: it reads the local store, not the server archive.
#[tauri::command]
pub async fn search_messages(
    state: State<'_, AppState>,
    peer: Option<String>,
    query: String,
    limit: Option<usize>,
) -> Res<Vec<chord_core::features::search::SearchHit>> {
    let peer = peer.map(|p| bare(&p)).transpose()?.map(|p| p.to_string());
    Ok(state
        .handle()?
        .search_messages(peer, query, limit.unwrap_or(50))
        .await?)
}

// ---------------------------------------------------------------- rooms

/// Join a room. With no nick, the room may have reserved one for us (XEP-0045, 7.12), else
/// the stored nick, then `fallback_nick`, then the local part of the JID is the nick. A
/// wrong or missing password fails with the code `notAuthorized`: ask for one and call
/// again.
#[tauri::command]
pub async fn join_room(
    state: State<'_, AppState>,
    room: String,
    nick: Option<String>,
    password: Option<String>,
    fallback_nick: Option<String>,
) -> Res<()> {
    let handle = state.handle()?;
    let room = bare(&room)?;
    match nick {
        Some(nick) => handle.join_room(room, nick, password).await?,
        None => {
            handle
                .join_room_default_nick(room, password, fallback_nick)
                .await?
        }
    }
    Ok(())
}

/// Set the subject of a room that we are in (XEP-0045, 8.1). An empty text clears it.
#[tauri::command]
pub async fn set_room_subject(
    state: State<'_, AppState>,
    room: String,
    subject: String,
) -> Res<()> {
    Ok(state
        .handle()?
        .set_room_subject(bare(&room)?, subject)
        .await?)
}

/// Set the role of an occupant by nick: none kicks, visitor mutes, participant gives
/// voice, moderator makes a moderator.
#[tauri::command]
pub async fn set_room_role(
    state: State<'_, AppState>,
    room: String,
    nick: String,
    role: RoomRole,
    reason: Option<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .set_room_role(bare(&room)?, nick, role, reason)
        .await?)
}

/// Read a room with a disco#info query. It does not join the room.
#[tauri::command]
pub async fn room_info(state: State<'_, AppState>, room: String) -> Res<RoomCard> {
    Ok(state.handle()?.room_info(bare(&room)?).await?)
}

/// Leave a room. The core retracts the bookmark of the room, if it has one.
#[tauri::command]
pub async fn leave_room(state: State<'_, AppState>, room: String) -> Res<()> {
    Ok(state.handle()?.leave_room(bare(&room)?).await?)
}

/// Save a room in the bookmarks of the account (XEP-0402). Only for a room that is in no
/// space: a space keeps its channels in its own node.
#[tauri::command]
pub async fn add_bookmark(
    state: State<'_, AppState>,
    room: String,
    name: Option<String>,
    autojoin: bool,
    nick: Option<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .add_bookmark(bare(&room)?, name, autojoin, nick)
        .await?)
}

/// Remove a room from the bookmarks. This does not leave the room.
#[tauri::command]
pub async fn remove_bookmark(state: State<'_, AppState>, room: String) -> Res<()> {
    Ok(state.handle()?.remove_bookmark(bare(&room)?).await?)
}

#[tauri::command]
pub async fn change_nick(state: State<'_, AppState>, room: String, nick: String) -> Res<()> {
    Ok(state.handle()?.change_nick(bare(&room)?, nick).await?)
}

/// Set our availability and status text. Offline, Chord only stores them.
#[tauri::command]
pub async fn set_presence(
    state: State<'_, AppState>,
    availability: chord_core::features::presence::Availability,
    status: Option<String>,
) -> Res<()> {
    Ok(state.handle()?.set_presence(availability, status).await?)
}

/// How the server can hide us: the invisible command (XEP-0186), a privacy list
/// (XEP-0016), or `None` when it has no invisible mode. Fails offline.
#[tauri::command]
pub async fn invisible_method(
    state: State<'_, AppState>,
) -> Res<Option<chord_core::features::presence::InvisibleMethod>> {
    Ok(state.handle()?.invisible_method().await?)
}

/// Tell the contacts that we are idle since `since` (Unix seconds), or no more idle
/// (`None`), with XEP-0319. Fails offline.
#[tauri::command]
pub async fn set_idle(state: State<'_, AppState>, since: Option<i64>) -> Res<()> {
    Ok(state.handle()?.set_idle(since).await?)
}

/// Our stored availability and status text.
#[tauri::command]
pub async fn own_presence(
    state: State<'_, AppState>,
) -> Res<chord_core::features::presence::OwnPresence> {
    Ok(state.handle()?.own_presence().await?)
}

/// The room service of the server, for example conference.example.org. New channels go
/// there. `None` when the server has none.
#[tauri::command]
pub async fn room_service(state: State<'_, AppState>) -> Res<Option<String>> {
    Ok(state
        .handle()?
        .room_service()
        .await?
        .map(|jid| jid.to_string()))
}

/// Send a private message to one occupant of a room. Returns its id.
#[tauri::command]
pub async fn send_private(
    state: State<'_, AppState>,
    room: String,
    nick: String,
    body: String,
) -> Res<String> {
    Ok(state
        .handle()?
        .send_private(bare(&room)?, nick, body)
        .await?)
}

#[tauri::command]
pub async fn set_room_affiliation(
    state: State<'_, AppState>,
    room: String,
    jid: String,
    affiliation: RoomAffiliation,
    reason: Option<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .set_room_affiliation(bare(&room)?, bare(&jid)?, affiliation, reason)
        .await?)
}

/// The JIDs with one affiliation in a room, each with its nick if the server gives one.
#[tauri::command]
pub async fn room_affiliations(
    state: State<'_, AppState>,
    room: String,
    affiliation: RoomAffiliation,
) -> Res<Vec<(BareJid, Option<String>)>> {
    Ok(state
        .handle()?
        .room_affiliations(bare(&room)?, affiliation)
        .await?)
}

#[tauri::command]
pub async fn invite_to_room(
    state: State<'_, AppState>,
    room: String,
    jid: String,
    reason: Option<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .invite_to_room(bare(&room)?, bare(&jid)?, reason)
        .await?)
}

/// Decline an invitation (`ClientEvent::RoomInvite`). To accept it, call `join_room`.
#[tauri::command]
pub async fn decline_room_invite(
    state: State<'_, AppState>,
    room: String,
    from: String,
    reason: Option<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .decline_room_invite(bare(&room)?, bare(&from)?, reason)
        .await?)
}

#[tauri::command]
pub async fn configure_room(
    state: State<'_, AppState>,
    room: String,
    settings: RoomSettings,
) -> Res<()> {
    Ok(state
        .handle()?
        .configure_room(bare(&room)?, settings)
        .await?)
}

// ---------------------------------------------------------------- spaces

#[tauri::command]
pub async fn browse_spaces(state: State<'_, AppState>) -> Res<Vec<SpaceInfo>> {
    Ok(state.handle()?.browse_spaces().await?)
}

/// Read a space (name, description, room count). It does not join the space.
#[tauri::command]
pub async fn space_info(
    state: State<'_, AppState>,
    service: String,
    node: String,
) -> Res<SpaceCard> {
    Ok(state.handle()?.space_info(&service, &node).await?)
}

#[tauri::command]
pub async fn join_space(
    state: State<'_, AppState>,
    service: String,
    node: String,
) -> Res<JoinOutcome> {
    Ok(state.handle()?.join_space(&service, &node).await?)
}

#[tauri::command]
pub async fn leave_space(state: State<'_, AppState>, service: String, node: String) -> Res<()> {
    Ok(state.handle()?.leave_space(&service, &node).await?)
}

/// Create a space. Returns its service and node.
#[tauri::command]
pub async fn create_space(
    state: State<'_, AppState>,
    name: String,
    access: SpaceAccess,
) -> Res<(String, String)> {
    Ok(state.handle()?.create_space_with(&name, access).await?)
}

#[tauri::command]
pub async fn delete_space(state: State<'_, AppState>, service: String, node: String) -> Res<()> {
    Ok(state.handle()?.delete_space(&service, &node).await?)
}

/// The joins of this account that wait for the owner: service, node, and name.
#[tauri::command]
pub async fn pending_space_joins(state: State<'_, AppState>) -> Res<Vec<PendingJoin>> {
    Ok(state.handle()?.pending_space_joins().await?)
}

/// The join requests that wait for us, as owner of a space.
#[tauri::command]
pub async fn space_join_requests(
    state: State<'_, AppState>,
    service: String,
    node: String,
) -> Res<Vec<JoinRequest>> {
    Ok(state.handle()?.space_join_requests(&service, &node).await?)
}

#[tauri::command]
pub async fn approve_space_join(
    state: State<'_, AppState>,
    service: String,
    node: String,
    jid: String,
) -> Res<()> {
    Ok(state
        .handle()?
        .approve_space_join(&service, &node, &jid)
        .await?)
}

#[tauri::command]
pub async fn deny_space_join(
    state: State<'_, AppState>,
    service: String,
    node: String,
    jid: String,
) -> Res<()> {
    Ok(state
        .handle()?
        .deny_space_join(&service, &node, &jid)
        .await?)
}

#[tauri::command]
pub async fn add_room_to_space(
    state: State<'_, AppState>,
    service: String,
    node: String,
    room: String,
    name: String,
) -> Res<()> {
    Ok(state
        .handle()?
        .add_room_to_space(&service, &node, bare(&room)?, &name)
        .await?)
}

#[tauri::command]
pub async fn remove_room_from_space(
    state: State<'_, AppState>,
    service: String,
    node: String,
    room: String,
) -> Res<()> {
    Ok(state
        .handle()?
        .remove_room_from_space(&service, &node, bare(&room)?)
        .await?)
}

#[tauri::command]
pub async fn add_space_member(
    state: State<'_, AppState>,
    service: String,
    node: String,
    member: String,
) -> Res<()> {
    Ok(state
        .handle()?
        .add_space_member(&service, &node, bare(&member)?)
        .await?)
}

// ---------------------------------------------------------------- contacts

#[tauri::command]
pub async fn contacts(state: State<'_, AppState>) -> Res<Vec<Contact>> {
    Ok(state.handle()?.contacts().await?)
}

#[tauri::command]
pub async fn add_contact(
    state: State<'_, AppState>,
    jid: String,
    name: Option<String>,
    preauth: Option<String>,
) -> Res<()> {
    let handle = state.handle()?;
    match preauth.filter(|t| !t.is_empty()) {
        Some(token) => {
            handle
                .add_contact_with_preauth(bare(&jid)?, name, token)
                .await?
        }
        None => handle.add_contact(bare(&jid)?, name).await?,
    }
    Ok(())
}

#[tauri::command]
pub async fn set_nickname(state: State<'_, AppState>, nickname: Option<String>) -> Res<()> {
    Ok(state.handle()?.set_nickname(nickname).await?)
}

#[tauri::command]
pub async fn profile(state: State<'_, AppState>, jid: String) -> Res<Profile> {
    Ok(state.handle()?.profile(bare(&jid)?).await?)
}

#[tauri::command]
pub async fn remove_contact(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.remove_contact(bare(&jid)?).await?)
}

/// Give a contact a new name in the roster. `None` or an empty name removes it.
#[tauri::command]
pub async fn rename_contact(
    state: State<'_, AppState>,
    jid: String,
    name: Option<String>,
) -> Res<()> {
    Ok(state.handle()?.rename_contact(bare(&jid)?, name).await?)
}

/// Replace the groups of a contact. An empty list clears them. The name stays.
#[tauri::command]
pub async fn set_contact_groups(
    state: State<'_, AppState>,
    jid: String,
    groups: Vec<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .set_contact_groups(bare(&jid)?, groups)
        .await?)
}

/// Accept the request of `ClientEvent::SubscriptionRequest`. With `add_back` it also asks
/// to see the presence of the contact, when we do not yet.
#[tauri::command]
pub async fn approve_subscription(
    state: State<'_, AppState>,
    jid: String,
    add_back: Option<bool>,
) -> Res<()> {
    Ok(state
        .handle()?
        .approve_subscription_with(bare(&jid)?, add_back.unwrap_or(false))
        .await?)
}

#[tauri::command]
pub async fn deny_subscription(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.deny_subscription(bare(&jid)?).await?)
}

#[tauri::command]
pub async fn preapprove_subscription(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.preapprove_subscription(bare(&jid)?).await?)
}

/// Block an address (XEP-0191). Fails with `unsupported` when the server cannot.
#[tauri::command]
pub async fn block_contact(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.block_contact(bare(&jid)?).await?)
}

/// Block an address and report it (XEP-0377). `reason` is "spam" or "abuse". A server that
/// does not take reports gets a plain block. Returns true when the block carried the report.
#[tauri::command]
pub async fn block_and_report(
    state: State<'_, AppState>,
    jid: String,
    reason: String,
) -> Res<bool> {
    let reason = match reason.as_str() {
        "spam" => chord_core::features::blocking::ReportReason::Spam,
        "abuse" => chord_core::features::blocking::ReportReason::Abuse,
        other => {
            return Err(ChordError::invalid(format!(
                "unknown report reason {other}"
            )));
        }
    };
    Ok(state
        .handle()?
        .block_and_report(bare(&jid)?, reason)
        .await?)
}

#[tauri::command]
pub async fn unblock_contact(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.unblock_contact(bare(&jid)?).await?)
}

#[tauri::command]
pub async fn unblock_all(state: State<'_, AppState>) -> Res<()> {
    Ok(state.handle()?.unblock_all().await?)
}

/// The blocked addresses. Works offline.
#[tauri::command]
pub async fn blocked_contacts(state: State<'_, AppState>) -> Res<Vec<BareJid>> {
    Ok(state.handle()?.blocked_contacts().await?)
}

// ---------------------------------------------------------------- avatars, levels, push

/// The largest avatar image that the UI may send, in bytes.
const MAX_AVATAR_BYTES: usize = 1024 * 1024;

/// Publish our avatar (XEP-0084, and the vCard photo for XEP-0153). The UI reads the
/// image and its size in pixels.
#[tauri::command]
pub async fn set_avatar(
    state: State<'_, AppState>,
    mime: String,
    data: Vec<u8>,
    width: u16,
    height: u16,
) -> Res<()> {
    if data.is_empty() || data.len() > MAX_AVATAR_BYTES || !mime.starts_with("image/") {
        return Err(ChordError::invalid("use an image under 1 MB"));
    }
    Ok(state
        .handle()?
        .set_avatar(mime, data, width, height)
        .await?)
}

/// Remove our avatar.
#[tauri::command]
pub async fn remove_avatar(state: State<'_, AppState>) -> Res<()> {
    Ok(state.handle()?.remove_avatar().await?)
}

/// Ask the server for the avatar of `owner`, and store it. The `chord-avatar` scheme then
/// shows it. Use it for a user that is not a contact.
#[tauri::command]
pub async fn refresh_avatar(state: State<'_, AppState>, owner: String) -> Res<()> {
    Ok(state.handle()?.refresh_avatar(bare(&owner)?).await?)
}

/// Set how much a chat or a room notifies. `mute_until` is a Unix time in ms.
#[tauri::command]
pub async fn set_notification_level(
    state: State<'_, AppState>,
    peer: String,
    level: NotificationLevel,
    mute_until: Option<i64>,
) -> Res<()> {
    Ok(state
        .handle()?
        .set_notification_level(peer, level, mute_until)
        .await?)
}

#[tauri::command]
pub async fn notification_level(
    state: State<'_, AppState>,
    peer: String,
) -> Res<NotificationSetting> {
    Ok(state.handle()?.notification_level(peer).await?)
}

#[tauri::command]
pub async fn push_registrations(state: State<'_, AppState>) -> Res<Vec<PushRegistration>> {
    Ok(state.handle()?.push_registrations().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saved_server_text_is_normal() {
        assert_eq!(server_key(None), "srv");
        assert_eq!(server_key(Some("  ")), "srv");
        assert_eq!(server_key(Some("srv")), "srv");
        assert_eq!(
            server_key(Some(" starttls://XMPP.Example.org:5222 ")),
            "starttls://xmpp.example.org:5222"
        );
        // What the keychain gives back must parse again.
        assert_eq!(
            parse_server(Some(&server_key(Some("starttls://Xmpp.Example.org:5222")))).unwrap(),
            ServerAddr::StartTls {
                host: "xmpp.example.org".into(),
                port: 5222
            }
        );
        assert_eq!(
            parse_server(Some(&server_key(None))).unwrap(),
            ServerAddr::Srv
        );
    }

    #[test]
    fn a_saved_password_goes_to_its_own_server_only() {
        let saved = |server: Option<&str>| {
            let server = server.map(str::to_owned);
            move || Ok((Some("secret".to_owned()), server))
        };
        // The UI names another host. The saved password ignores it.
        let got = credentials(None, Some("starttls://evil.example:5222"), saved(None)).unwrap();
        assert_eq!(got, ("secret".into(), "srv".into()));
        let got = credentials(
            Some(String::new()),
            Some("starttls://evil.example:5222"),
            saved(Some("starttls://xmpp.example.org:5222")),
        )
        .unwrap();
        assert_eq!(
            got,
            ("secret".into(), "starttls://xmpp.example.org:5222".into())
        );
        // A typed password uses the typed server and never reads the keychain.
        let got = credentials(
            Some("typed".into()),
            Some("starttls://Mine.example:5222"),
            || panic!("the keychain is not read"),
        )
        .unwrap();
        assert_eq!(got, ("typed".into(), "starttls://mine.example:5222".into()));
        // No password anywhere is an error.
        assert!(credentials(None, None, || Ok((None, None))).is_err());
    }

    #[test]
    fn server_field_parses() {
        assert_eq!(parse_server(None).unwrap(), ServerAddr::Srv);
        assert_eq!(parse_server(Some("")).unwrap(), ServerAddr::Srv);
        assert_eq!(parse_server(Some("  ")).unwrap(), ServerAddr::Srv);
        assert_eq!(
            parse_server(Some("starttls://xmpp.example.org:5222")).unwrap(),
            ServerAddr::StartTls {
                host: "xmpp.example.org".into(),
                port: 5222
            }
        );
        assert_eq!(
            parse_server(Some("xmpps://xmpp.example.org:5223")).unwrap(),
            ServerAddr::DirectTls {
                host: "xmpp.example.org".into(),
                port: 5223
            }
        );
    }

    #[test]
    fn bad_server_fields_fail() {
        for bad in [
            "xmpp.example.org:5222",
            "tcp://localhost:5222",
            "starttls://host",
            "starttls://host:0",
            "starttls://host:99999",
            "starttls://:5222",
            "starttls://a@host:5222",
        ] {
            let error = parse_server(Some(bad)).unwrap_err();
            assert_eq!(error.code, "invalid", "{bad}");
        }
    }

    #[test]
    fn bad_jids_give_the_invalid_code() {
        assert_eq!(bare("not a jid").unwrap_err().code, "invalid");
        assert_eq!(bare("amy@example.org/res").unwrap_err().code, "invalid");
        assert!(bare("amy@example.org").is_ok());
        assert!(full("amy@example.org/res").is_ok());
    }

    #[test]
    fn content_types_follow_the_extension() {
        assert_eq!(guess_content_type("a.PNG"), "image/png");
        assert_eq!(guess_content_type("a.tar.gz"), "application/octet-stream");
        assert_eq!(guess_content_type("noext"), "application/octet-stream");
    }
}
