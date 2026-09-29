//! The commands of the UI. Each one calls one `ClientHandle` method.
//!
//! JIDs arrive as strings and are parsed here. Errors leave as `ChordError`. The
//! TypeScript wrappers are in src/lib/chord/api.ts, with the same names in camelCase.

use std::path::PathBuf;

use chord_core::actor;
use chord_core::actor::ClientEvent;
use chord_core::features::muc::{RoomAffiliation, RoomSettings};
use chord_core::features::notify::{NotificationLevel, NotificationSetting};
use chord_core::features::push::PushRegistration;
use chord_core::features::roster::Contact;
use chord_core::features::spaces::{JoinOutcome, JoinRequest, PendingJoin, SpaceAccess, SpaceInfo};
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

/// The server field of `login`: none or empty for a SRV lookup, or `starttls://host:port`.
pub fn parse_server(server: Option<&str>) -> Res<ServerAddr> {
    let bad = |s: &str| {
        ChordError::invalid(format!(
            "the server must be empty or starttls://host:port, not {s:?}"
        ))
    };
    let s = match server.map(str::trim) {
        None | Some("") | Some("srv") => return Ok(ServerAddr::Srv),
        Some(s) => s,
    };
    let rest = s.strip_prefix("starttls://").ok_or_else(|| bad(s))?;
    let (host, port) = rest.rsplit_once(':').ok_or_else(|| bad(s))?;
    let port: u16 = port.parse().map_err(|_| bad(s))?;
    if host.is_empty() || host.contains(['/', '@', ' ']) || port == 0 {
        return Err(bad(s));
    }
    Ok(ServerAddr::StartTls {
        host: host.to_owned(),
        port,
    })
}

/// The file name of an upload, from its path.
fn file_name(path: &std::path::Path) -> Res<String> {
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

/// Log in. `password` is optional: with none, the saved password is used. `server` is
/// empty for a SRV lookup, or `starttls://host:port`. With `remember`, a good login
/// saves the password in the system keychain.
#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    password: Option<String>,
    server: Option<String>,
    remember: Option<bool>,
) -> Res<()> {
    let client = state.client()?;
    let server = parse_server(server.as_deref())?;
    let password = match password.filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => keychain::get(client.account.as_str())?
            .ok_or_else(|| ChordError::invalid("no password given and none saved"))?,
    };
    let config = SessionConfig::new(client.account.clone(), password.clone(), server);
    client.handle.login(config).await?;
    if remember.unwrap_or(false) {
        keychain::set(client.account.as_str(), &password)?;
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

/// Upload the file at `path` (XEP-0363) and send its URL to `to`. Rust reads the file.
/// Returns the URL. `content_type` is optional: the extension gives the default.
#[tauri::command]
pub async fn upload(
    state: State<'_, AppState>,
    to: String,
    path: String,
    content_type: Option<String>,
) -> Res<String> {
    let handle = state.handle()?;
    let to = full(&to)?;
    let path = PathBuf::from(path);
    let name = file_name(&path)?;
    let data = tauri::async_runtime::spawn_blocking(move || read_capped(&path))
        .await
        .map_err(|e| ChordError::io("the read task failed", e))??;
    let content_type = content_type
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| guess_content_type(&name).to_owned());
    Ok(handle.upload(to, name, content_type, data).await?)
}

fn read_capped(path: &std::path::Path) -> Res<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|e| ChordError::io("cannot read the file", e))?;
    if !meta.is_file() {
        return Err(ChordError::invalid("the path is not a file"));
    }
    if meta.len() > MAX_UPLOAD_BYTES {
        return Err(ChordError::invalid(format!(
            "the file is {} bytes, the limit is {MAX_UPLOAD_BYTES}",
            meta.len()
        )));
    }
    std::fs::read(path).map_err(|e| ChordError::io("cannot read the file", e))
}

/// Fetch older messages of a chat or a room from the archive (MAM).
#[tauri::command]
pub async fn load_older(state: State<'_, AppState>, peer: String) -> Res<()> {
    Ok(state.handle()?.load_older(bare(&peer)?).await?)
}

// ---------------------------------------------------------------- rooms

#[tauri::command]
pub async fn join_room(
    state: State<'_, AppState>,
    room: String,
    nick: String,
    password: Option<String>,
) -> Res<()> {
    Ok(state
        .handle()?
        .join_room(bare(&room)?, nick, password)
        .await?)
}

#[tauri::command]
pub async fn leave_room(state: State<'_, AppState>, room: String) -> Res<()> {
    Ok(state.handle()?.leave_room(bare(&room)?).await?)
}

#[tauri::command]
pub async fn change_nick(state: State<'_, AppState>, room: String, nick: String) -> Res<()> {
    Ok(state.handle()?.change_nick(bare(&room)?, nick).await?)
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
pub async fn add_contact(state: State<'_, AppState>, jid: String, name: Option<String>) -> Res<()> {
    Ok(state.handle()?.add_contact(bare(&jid)?, name).await?)
}

#[tauri::command]
pub async fn remove_contact(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.remove_contact(bare(&jid)?).await?)
}

/// Accept the request of `ClientEvent::SubscriptionRequest`.
#[tauri::command]
pub async fn approve_subscription(state: State<'_, AppState>, jid: String) -> Res<()> {
    Ok(state.handle()?.approve_subscription(bare(&jid)?).await?)
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

    #[test]
    fn upload_reads_only_small_files() {
        let error = read_capped(std::path::Path::new("/definitely/not/here")).unwrap_err();
        assert_eq!(error.code, "io");
        let dir = std::env::temp_dir();
        assert_eq!(read_capped(&dir).unwrap_err().code, "invalid");
    }
}
