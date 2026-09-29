// The one place that maps bridge types ($lib/chord/types) to the UI types (./types).
// The functions are pure. The live controller (live.svelte.ts) calls them.
import { avatarUrl } from '$lib/chord/avatars';
import type {
  ChannelItem as BChannel,
  ChannelScope,
  ConnectError,
  AuthFailure,
  Contact as BContact,
  ListDiff,
  MemberItem as BMember,
  NotificationLevel as BLevel,
  NotificationSetting,
  SpaceInfo,
  SpaceItem as BSpace,
  TimelineItem as BTimeline
} from '$lib/chord/types';
import {
  spaceKey,
  type Affiliation,
  type Attachment,
  type ChannelItem,
  type ContactItem,
  type MemberItem,
  type NotificationLevel,
  type PublicCircle,
  type Show,
  type SpaceItem,
  type TimelineItem
} from './types';

// --- names and keys --------------------------------------------------

export function localPart(jid: string): string {
  return jid.split('/')[0].split('@')[0];
}

/** `service/node`, the key of a circle in the UI. */
export function splitSpaceKey(key: string): { service: string; node: string } {
  const i = key.indexOf('/');
  return { service: key.slice(0, i), node: key.slice(i + 1) };
}

export function scopeOf(key: string | null): ChannelScope {
  if (!key) return { type: 'home' };
  return { type: 'space', ...splitSpaceKey(key) };
}

/** Split `room/nick`. Null for a bare JID. */
export function splitPrivate(jid: string): { room: string; nick: string } | null {
  const i = jid.indexOf('/');
  return i < 0 ? null : { room: jid.slice(0, i), nick: jid.slice(i + 1) };
}

const avatar = (hash: string | null): string | null => (hash ? avatarUrl(hash) : null);

// --- notification levels ---------------------------------------------

export function levelToUi(level: BLevel): NotificationLevel {
  return level === 'none' ? 'nothing' : level;
}

export function levelToBridge(level: NotificationLevel): BLevel {
  return level === 'nothing' ? 'none' : level;
}

/** A peer that shows no notices: level "none", or a mute time in the future. */
export function isMuted(s: NotificationSetting | undefined, now = Date.now()): boolean {
  if (!s) return false;
  return s.level === 'none' || (s.muteUntil !== null && s.muteUntil > now);
}

// --- views -----------------------------------------------------------

export function toSpace(s: BSpace): SpaceItem {
  return { service: s.service, node: s.node, name: s.name, avatar: avatar(s.avatar) };
}

export interface ChannelContext {
  /** Key of the circle whose list holds the item, or null for Home. */
  space: string | null;
  muted: boolean;
  mentions: number;
  /** What the open chat told us about the peer. Null: unknown. */
  presence: { show: Show; online: boolean } | null;
}

export function toChannel(c: BChannel, ctx: ChannelContext): ChannelItem {
  const pm = c.kind.type === 'privateMessage' ? { room: c.kind.room, nick: c.kind.nick } : null;
  const direct = c.kind.type === 'direct';
  return {
    jid: c.jid,
    name: pm ? `${pm.nick} in ${localPart(pm.room)}` : c.name,
    kind: c.kind.type === 'room' ? 'channel' : 'dm',
    unread: c.unread,
    joined: c.joined,
    mentions: ctx.mentions,
    muted: ctx.muted,
    topic: null,
    space: c.kind.type === 'room' ? ctx.space : null,
    // The avatar of a chat is stored under the JID of the peer.
    avatar: direct ? avatarUrl(c.jid) : null,
    show: ctx.presence?.show ?? null,
    online: ctx.presence?.online ?? false,
    pm,
    unknownPresence: !ctx.presence
  };
}

const SHOWS: readonly string[] = ['chat', 'away', 'xa', 'dnd'];
const AFFILIATIONS: readonly string[] = ['owner', 'admin', 'member', 'none'];

export function toShow(show: string | null): Show {
  return show !== null && SHOWS.includes(show) ? (show as Show) : null;
}

/**
 * A member. In a room the UI id is the real JID, or `room/nick` when the room hides it,
 * so an address always works for a profile, a chat, or a private message.
 */
export function toMember(m: BMember, room: string): MemberItem {
  const affiliation = AFFILIATIONS.includes(m.affiliation) ? (m.affiliation as Affiliation) : 'none';
  return {
    id: m.jid ?? `${room}/${m.id}`,
    name: m.name,
    role: m.role === 'moderator' ? 'Moderator' : null,
    affiliation,
    show: toShow(m.show),
    online: m.online,
    avatar: avatar(m.avatar),
    jid: m.jid,
    nick: m.id
  };
}

const MIMES: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  svg: 'image/svg+xml',
  mp4: 'video/mp4',
  webm: 'video/webm',
  mp3: 'audio/mpeg',
  ogg: 'audio/ogg',
  pdf: 'application/pdf',
  txt: 'text/plain'
};

/** The bridge gives a URL. The name and the type come from the URL. */
export function toAttachment(url: string): Attachment {
  let path = url;
  try {
    path = new URL(url).pathname;
  } catch {
    /* not a full URL, use it as it is */
  }
  const last = path.split('/').pop() || url;
  let name = last;
  try {
    name = decodeURIComponent(last);
  } catch {
    /* keep the raw name */
  }
  const ext = name.includes('.') ? name.split('.').pop()!.toLowerCase() : '';
  return {
    url,
    name,
    mime: MIMES[ext] ?? 'application/octet-stream',
    size: 0,
    width: null,
    height: null
  };
}

export interface TimelineContext {
  /** The room timelines have a `room/nick` sender. */
  room: boolean;
  me: string;
  /** The real JID of a nick in this room, if the member list has it. */
  resolve?: (nick: string) => string | null;
}

export function toTimelineItem(t: BTimeline, ctx: TimelineContext): TimelineItem {
  let sender: string;
  if (t.outgoing) sender = ctx.me;
  else if (ctx.room) {
    const nick = t.sender.includes('/') ? t.sender.slice(t.sender.indexOf('/') + 1) : t.sender;
    sender = ctx.resolve?.(nick) ?? t.sender;
  } else sender = t.sender.split('/')[0];
  return {
    id: t.id,
    sender,
    senderName: t.senderName,
    avatar: avatar(t.avatar),
    body: t.body,
    timestamp: t.timestamp,
    outgoing: t.outgoing,
    sameSenderAsPrevious: t.sameSenderAsPrevious,
    edited: t.edited,
    retracted: t.retracted,
    reactions: t.reactions.map((r) => ({ emoji: r.emoji, count: r.count, mine: r.mine })),
    replyTo: t.replyTo
      ? { id: t.replyTo.id ?? '', senderName: t.replyTo.senderName, body: t.replyTo.body }
      : null,
    attachment: t.attachment ? toAttachment(t.attachment) : null,
    // The UI has no "received" or "displayed" mark.
    status: 'sent',
    // The bridge tells about mentions in Notification events. The store adds the flag.
    mention: false
  };
}

/** Map the items of a diff. */
export function mapDiff<A, B>(diff: ListDiff<A>, f: (a: A) => B): ListDiff<B> {
  switch (diff.type) {
    case 'reset':
      return { type: 'reset', items: diff.items.map(f) };
    case 'insert':
      return { type: 'insert', index: diff.index, item: f(diff.item) };
    case 'update':
      return { type: 'update', index: diff.index, item: f(diff.item) };
    case 'remove':
      return diff;
  }
}

// --- contacts --------------------------------------------------------

export function toContactItem(jid: string, name: string | null, since: number | null = null): ContactItem {
  return {
    address: jid,
    name: name || localPart(jid),
    avatar: avatarUrl(jid),
    // The bridge has no roster presence.
    show: null,
    online: false,
    status: null,
    since
  };
}

export interface ContactLists {
  contacts: ContactItem[];
  outgoing: ContactItem[];
}

/** Split the roster. A request that we sent and nobody answered is outgoing. */
export function splitRoster(list: BContact[]): ContactLists {
  const out: ContactLists = { contacts: [], outgoing: [] };
  for (const c of list) {
    if (c.blocked) continue;
    const item = toContactItem(c.jid, c.name);
    if (c.ask && (c.subscription === 'none' || c.subscription === 'from')) out.outgoing.push(item);
    else out.contacts.push(item);
  }
  return out;
}

export function toPublicCircle(i: SpaceInfo): PublicCircle {
  return {
    service: i.service,
    node: i.node,
    name: i.name || i.node,
    description: i.description ?? '',
    members: null
  };
}

export { spaceKey };

// --- errors ----------------------------------------------------------

/** Plain words for an error of the bridge. */
export function plainError(error: unknown): string {
  const e = error as { code?: string; message?: string } | null;
  const message = typeof e?.message === 'string' ? e.message : String(error);
  switch (e?.code) {
    case 'authFailed':
      return 'Wrong address or password.';
    case 'unreachable':
      return "Can't reach the server. Check the address and your connection.";
    case 'tlsInvalid':
      return "The server's certificate is not valid, so Chord did not connect.";
    case 'timeout':
      return 'The server did not answer in time. Try again.';
    case 'notConnected':
      return 'You are offline. Try again when the connection is back.';
    case 'unsupported':
      return 'Your server does not support this.';
    case 'keychain':
      return `Chord could not use the system keychain. ${message}`;
    case 'store':
      return `Chord could not read its data. ${message}`;
    default:
      return message;
  }
}

export function authFailureText(f: AuthFailure): string {
  if (f.type === 'noMechanism') return 'The server offers no sign-in method that Chord can use.';
  if (f.type === 'sasl') return 'Wrong address or password.';
  return `Sign-in failed. ${f.data}`;
}

export function connectErrorText(e: ConnectError, host: string): string {
  switch (e.type) {
    case 'authFailed':
      return authFailureText(e.data);
    case 'unreachable':
      return `Can't reach ${host}. Check the address and your connection.`;
    case 'tlsInvalid':
      return `The certificate of ${host} is not valid, so Chord did not connect.`;
    case 'timeout':
      return `${host} did not answer in time. Try again.`;
  }
}
