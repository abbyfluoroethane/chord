// Typed wrappers for the Tauri commands (src-tauri/src/commands.rs). One function per
// command. Each one returns a promise that rejects with a `ChordError`.

import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  Availability,
  ChannelItem,
  ChannelScope,
  ChordError,
  ClientEvent,
  Contact,
  EmojiPackStatus,
  GifPage,
  Jid,
  JoinOutcome,
  JoinRequest,
  LinkPreview,
  ListDiff,
  MemberItem,
  NotificationLevel,
  NotificationSetting,
  OpenInfo,
  OwnPresence,
  PendingJoin,
  Pin,
  PushRegistration,
  RoomAffiliation,
  RoomCard,
  RoomSettings,
  SearchHit,
  Settings,
  SpaceAccess,
  SpaceCard,
  SpaceInfo,
  SpaceItem,
  TimelineItem
} from './types';

/** True if `error` has the shape of a `ChordError`. */
export function isChordError(error: unknown): error is ChordError {
  return (
    typeof error === 'object' &&
    error !== null &&
    typeof (error as ChordError).code === 'string' &&
    typeof (error as ChordError).message === 'string'
  );
}

/** The text to show for a rejected command. */
export function errorMessage(error: unknown): string {
  if (isChordError(error)) return error.message;
  return error instanceof Error ? error.message : String(error);
}

// ---------------------------------------------------------------- account

/** Open the account on its database. Works offline. Call it once, before all else. */
export const open = (account: string) => invoke<OpenInfo>('open', { account });

/**
 * Log in. With no `password`, the saved one is used. `server`: empty for a SRV lookup,
 * `starttls://host:port`, or `xmpps://host:port` for direct TLS. With `remember`, a good login saves the password in the
 * system keychain.
 */
export const login = (opts: { password?: string; server?: string; remember?: boolean } = {}) =>
  invoke<void>('login', {
    password: opts.password ?? null,
    server: opts.server ?? null,
    remember: opts.remember ?? false,
  });

export const logout = () => invoke<void>('logout');
export const savedPassword = (account: string) => invoke<boolean>('saved_password', { account });
export const forgetPassword = (account: string) => invoke<void>('forget_password', { account });

// ---------------------------------------------------------------- events

/**
 * Listen to the client events: connection state, notifications, typing, invites, notices.
 * There is one listener: a second call replaces the first. Events from before the call
 * arrive first (up to 200).
 */
export function listenEvents(onEvent: (event: ClientEvent) => void): Promise<void> {
  const channel = new Channel<ClientEvent>();
  channel.onmessage = onEvent;
  return invoke<void>('events', { onEvent: channel });
}

// ---------------------------------------------------------------- views

/** A running view subscription. */
export interface ViewSubscription {
  readonly id: number;
  /** Stop the subscription. Safe to call twice. */
  unsubscribe(): Promise<void>;
}

/** A timeline subscription. It can also load older messages. */
export interface TimelineSubscription extends ViewSubscription {
  /** Show `count` older messages. The diffs arrive through the same callback. */
  paginateBack(count: number): Promise<void>;
}

function makeSubscription(id: number): ViewSubscription {
  let stopped = false;
  return {
    id,
    async unsubscribe() {
      if (stopped) return;
      stopped = true;
      await invoke<void>('unsubscribe', { id });
    },
  };
}

async function subscribe<T>(
  command: string,
  args: Record<string, unknown>,
  onDiff: (diff: ListDiff<T>) => void,
): Promise<ViewSubscription> {
  const channel = new Channel<ListDiff<T>>();
  channel.onmessage = onDiff;
  const id = await invoke<number>(command, { ...args, onDiff: channel });
  return makeSubscription(id);
}

/** The spaces of the rail. The first diff is a `reset`. */
export const subscribeSpaceList = (onDiff: (diff: ListDiff<SpaceItem>) => void) =>
  subscribe<SpaceItem>('subscribe_space_list', {}, onDiff);

/** The channels of Home or of one space. */
export const subscribeChannelList = (
  scope: ChannelScope,
  onDiff: (diff: ListDiff<ChannelItem>) => void,
) => subscribe<ChannelItem>('subscribe_channel_list', { scope }, onDiff);

/** The members of a room, or the two people of a chat. */
export const subscribeMemberList = (room: string, onDiff: (diff: ListDiff<MemberItem>) => void) =>
  subscribe<MemberItem>('subscribe_member_list', { room }, onDiff);

async function timelineSubscription(
  command: string,
  args: Record<string, unknown>,
  onDiff: (diff: ListDiff<TimelineItem>) => void,
): Promise<TimelineSubscription> {
  const base = await subscribe<TimelineItem>(command, args, onDiff);
  return {
    ...base,
    paginateBack: (count: number) => invoke<void>('timeline_paginate_back', { id: base.id, count }),
  };
}

/**
 * The messages of a room or a chat (`peer` is a bare JID). The first diff is a `reset`
 * with the newest 50. Use `applyDiff` to keep a list. Call `unsubscribe` when the view
 * goes away.
 */
export const subscribeTimeline = (peer: string, onDiff: (diff: ListDiff<TimelineItem>) => void) =>
  timelineSubscription('subscribe_timeline', { peer }, onDiff);

/** The private messages with one occupant of a room. */
export const subscribePrivateTimeline = (
  room: string,
  nick: string,
  onDiff: (diff: ListDiff<TimelineItem>) => void,
) => timelineSubscription('subscribe_private_timeline', { room, nick }, onDiff);

// ---------------------------------------------------------------- messages

/** Send a chat message. Returns its origin-id. */
export const sendChat = (to: string, body: string) => invoke<string>('send_chat', { to, body });
/** Send an https link that clients show inline (XEP-0066), for example a GIF. */
export const sendLink = (to: string, url: string) => invoke<string>('send_link', { to, url });
export const editMessage = (itemId: string, body: string) =>
  invoke<void>('edit_message', { itemId, body });
export const retractMessage = (itemId: string) => invoke<void>('retract_message', { itemId });
/** Delete the message of another user, as a room moderator. */
export const moderateMessage = (itemId: string, reason?: string) =>
  invoke<void>('moderate_message', { itemId, reason: reason ?? null });
export const reply = (itemId: string, body: string) => invoke<void>('reply', { itemId, body });
/** Replace our reactions to a message with `emojis`. */
export const react = (itemId: string, emojis: string[]) => invoke<void>('react', { itemId, emojis });
export const toggleReaction = (itemId: string, emoji: string) =>
  invoke<void>('toggle_reaction', { itemId, emoji });
export const markRead = (peer: string) => invoke<void>('mark_read', { peer });
/** Mark the message `itemId` and the later ones as unread. Sends nothing to the server. */
export const markUnread = (itemId: string) => invoke<void>('mark_unread', { itemId });
export const markReadPrivate = (room: string, nick: string) =>
  invoke<void>('mark_read_private', { room, nick });
/** Tell the peer that we type. See `ClientEvent` `typing` for what `peer` is. */
export const setTyping = (peer: string, typing: boolean) =>
  invoke<void>('set_typing', { peer, typing });
/**
 * Tell the server that the window is in use or not (XEP-0352). The server then holds back
 * presence and chat states. Resolves to true when the server got it. Core sends the state
 * again after a reconnect.
 */
export const setClientActive = (active: boolean) => invoke<boolean>('set_client_active', { active });
/**
 * Open the system file dialog and upload the files that the user picks (100 MB each at
 * most) to `to`. Rust runs the dialog and reads the files: the page never gives a path.
 * Returns the URLs. The list is empty when the user cancels.
 */
export const uploadFiles = (to: string) => invoke<string[]>('upload_files', { to });
/**
 * Upload a file that the user dropped on the window. `path` must come from the drop event
 * (`onDragDropEvent`): Rust refuses a path that no drop gave, and accepts each one once.
 * Returns the URL.
 */
export const uploadDropped = (to: string, path: string) =>
  invoke<string>('upload_dropped', { to, path });
/**
 * Upload a file that the user pasted: the page has the bytes and no path. The bytes go
 * to Rust as the raw body (25 MB at most). Rust makes the file name from the media type.
 * Returns the URL.
 */
export const uploadPasted = (to: string, type: string, bytes: Uint8Array) =>
  invoke<string>('upload_pasted', bytes, {
    headers: { to: encodeURIComponent(to), type: encodeURIComponent(type) }
  });
/** Fetch older messages of a chat or a room from the server archive. */
export const loadOlder = (peer: string) => invoke<void>('load_older', { peer });

/** Search the stored messages, newest first. Without `peer` it searches every chat. */
export const searchMessages = (query: string, peer?: string, limit = 50) =>
  invoke<SearchHit[]>('search_messages', { peer: peer ?? null, query, limit });

// ---------------------------------------------------------------- rooms

export const joinRoom = (room: string, nick: string, password?: string) =>
  invoke<void>('join_room', { room, nick, password: password ?? null });
/** Read a room with a disco#info query. It does not join the room. */
export const roomInfo = (room: string) => invoke<RoomCard>('room_info', { room });
export const leaveRoom = (room: string) => invoke<void>('leave_room', { room });
/** Save a room in the account bookmarks (XEP-0402). Not for a channel of a space. */
export const addBookmark = (room: string, nick: string, name: string | null = null) =>
  invoke<void>('add_bookmark', { room, name, autojoin: true, nick });
export const removeBookmark = (room: string) => invoke<void>('remove_bookmark', { room });
export const changeNick = (room: string, nick: string) =>
  invoke<void>('change_nick', { room, nick });
/** Set our availability and status text. Offline, Chord only stores them. */
export const setPresence = (availability: Availability, status: string | null) =>
  invoke<void>('set_presence', { availability, status });
/** Our stored availability and status text. */
export const ownPresence = () => invoke<OwnPresence>('own_presence');
/** The room service of the server (for example conference.example.org), or null. */
export const roomService = () => invoke<string | null>('room_service');
export const sendPrivate = (room: string, nick: string, body: string) =>
  invoke<string>('send_private', { room, nick, body });
export const setRoomAffiliation = (
  room: string,
  jid: string,
  affiliation: RoomAffiliation,
  reason?: string,
) => invoke<void>('set_room_affiliation', { room, jid, affiliation, reason: reason ?? null });
/** The JIDs with one affiliation, each with its nick if the server gives one. */
export const roomAffiliations = (room: string, affiliation: RoomAffiliation) =>
  invoke<[string, string | null][]>('room_affiliations', { room, affiliation });
export const inviteToRoom = (room: string, jid: string, reason?: string) =>
  invoke<void>('invite_to_room', { room, jid, reason: reason ?? null });
export const declineRoomInvite = (room: string, from: string, reason?: string) =>
  invoke<void>('decline_room_invite', { room, from, reason: reason ?? null });
export const configureRoom = (room: string, settings: RoomSettings) =>
  invoke<void>('configure_room', { room, settings });

// ---------------------------------------------------------------- spaces

export const browseSpaces = () => invoke<SpaceInfo[]>('browse_spaces');
/** Read a space (name, description, room count). It does not join the space. */
export const spaceInfo = (service: string, node: string) =>
  invoke<SpaceCard>('space_info', { service, node });
export const joinSpace = (service: string, node: string) =>
  invoke<JoinOutcome>('join_space', { service, node });
export const leaveSpace = (service: string, node: string) =>
  invoke<void>('leave_space', { service, node });
/** Create a space. Returns `[service, node]`. */
export const createSpace = (name: string, access: SpaceAccess) =>
  invoke<[string, string]>('create_space', { name, access });
export const deleteSpace = (service: string, node: string) =>
  invoke<void>('delete_space', { service, node });
export const pendingSpaceJoins = () => invoke<PendingJoin[]>('pending_space_joins');
export const spaceJoinRequests = (service: string, node: string) =>
  invoke<JoinRequest[]>('space_join_requests', { service, node });
export const approveSpaceJoin = (service: string, node: string, jid: string) =>
  invoke<void>('approve_space_join', { service, node, jid });
export const denySpaceJoin = (service: string, node: string, jid: string) =>
  invoke<void>('deny_space_join', { service, node, jid });
export const addRoomToSpace = (service: string, node: string, room: string, name: string) =>
  invoke<void>('add_room_to_space', { service, node, room, name });
export const removeRoomFromSpace = (service: string, node: string, room: string) =>
  invoke<void>('remove_room_from_space', { service, node, room });
export const addSpaceMember = (service: string, node: string, member: string) =>
  invoke<void>('add_space_member', { service, node, member });

// ---------------------------------------------------------------- contacts

export const contacts = () => invoke<Contact[]>('contacts');

/** XEP-0191. Fails with an `unsupported` error when the server has no blocking support. */
export const blockContact = (jid: string) => invoke<void>('block_contact', { jid });
export const unblockContact = (jid: string) => invoke<void>('unblock_contact', { jid });
export const unblockAll = () => invoke<void>('unblock_all');
/** Reads the stored copy of the blocklist, so it works offline. */
export const blockedContacts = () => invoke<Jid[]>('blocked_contacts');
export const addContact = (jid: string, name?: string) =>
  invoke<void>('add_contact', { jid, name: name ?? null });
export const removeContact = (jid: string) => invoke<void>('remove_contact', { jid });
/** Accept the request of a `subscriptionRequest` event. */
export const approveSubscription = (jid: string) => invoke<void>('approve_subscription', { jid });
export const denySubscription = (jid: string) => invoke<void>('deny_subscription', { jid });
export const preapproveSubscription = (jid: string) =>
  invoke<void>('preapprove_subscription', { jid });

// ---------------------------------------------------------------- avatars, levels, push

/** Ask the server for the avatar of `owner` and store it. `avatarUrl` then shows it. */
export const refreshAvatar = (owner: string) => invoke<void>('refresh_avatar', { owner });
/** Publish our avatar. `width` and `height` are the image size in pixels. */
export const setAvatar = (mime: string, data: Uint8Array, width: number, height: number) =>
  invoke<void>('set_avatar', { mime, data: Array.from(data), width, height });
export const removeAvatar = () => invoke<void>('remove_avatar');
/** `muteUntil` is a Unix time in ms. */
export const setNotificationLevel = (peer: string, level: NotificationLevel, muteUntil?: number) =>
  invoke<void>('set_notification_level', { peer, level, muteUntil: muteUntil ?? null });
export const notificationLevel = (peer: string) =>
  invoke<NotificationSetting>('notification_level', { peer });
export const pushRegistrations = () => invoke<PushRegistration[]>('push_registrations');

// ---------------------------------------------------------------- pins

/** Pin a message (`itemId` is its timeline id). The pin goes to the account, for all devices. */
export const pinMessage = (itemId: string) => invoke<void>('pin_message', { itemId });
/** Remove a pin. `chat` and `key` are the fields of the `Pin`. */
export const unpinMessage = (chat: string, key: string) =>
  invoke<void>('unpin_message', { chat, key });
/** The pins of one chat, newest first, from the local copy. */
export const listPins = (chat: string) => invoke<Pin[]>('pins', { chat });
/** Ask the server for the pins again. The core also does it at each connect. */
export const refreshPins = () => invoke<void>('refresh_pins');

// ---------------------------------------------------------------- local settings

/** The local settings (for example space folders), or `{}`. */
export const getSettings = () => invoke<Settings>('get_settings');
/** Replace the local settings. Any JSON, 256 KB at most. */
export const setSettings = (value: Settings) => invoke<void>('set_settings', { value });

// ---------------------------------------------------------------- link previews

/**
 * Fetch the page of `url` from this computer and read its title, text and image. The
 * site sees our IP address. Resolves to null when the page has no preview. Rust caches
 * the answer for one hour and refuses private addresses.
 */
/**
 * Download the image at `url` (public addresses only, 50 MB at most), then open the save
 * dialog with `name` as the suggestion. Rust writes the file where the user says. Resolves
 * to false when the user cancels.
 */
export const saveImage = (url: string, name?: string) =>
  invoke<boolean>('save_image', { url, name: name ?? null });
export const linkPreview = (url: string) => invoke<LinkPreview | null>('link_preview', { url });
/** Download the CSS of a linked theme. Https only, 256 KB at most. */
export const themeFetch = (url: string) => invoke<string>('theme_fetch', { url });
/** Search KLIPY GIFs, or get the trending ones for an empty query. `page` starts at 1. */
export const gifSearch = (query: string, page: number) =>
  invoke<GifPage>('gif_search', { query, page });
/** The emoji packs and whether each one is on this computer. */
export const emojiPacks = () => invoke<EmojiPackStatus[]>('emoji_packs');
/** Install an emoji pack. Noto and Fluent download from the npm registry. */
export const emojiPackInstall = (id: string) => invoke<void>('emoji_pack_install', { id });
