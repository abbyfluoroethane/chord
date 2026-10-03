// Local view types. They mirror the core view items and get swapped for the
// bridge types later. Fields marked "UI" are extra and the bridge may derive them.

/** 'invisible' is only ours: contacts see us as offline. */
export type Show = 'chat' | 'away' | 'xa' | 'dnd' | 'invisible' | null;
export type PresenceKind = 'online' | 'away' | 'dnd' | 'offline';
export type Affiliation = 'owner' | 'admin' | 'member' | 'none';
export type MessageStatus = 'sending' | 'sent' | 'failed';

export interface SpaceItem {
  service: string;
  node: string;
  name: string;
  avatar: string | null;
}

export interface ChannelItem {
  jid: string;
  name: string;
  kind: 'channel' | 'dm';
  unread: number;
  joined: boolean;
  mentions: number; // UI: mention count
  muted: boolean; // UI
  topic: string | null; // UI
  space: string | null; // UI: SpaceItem key, null for DMs
  avatar: string | null; // UI: DMs
  show: Show; // UI: DMs
  online: boolean; // UI: DMs
  /** The private messages with one room member. Set for a `room/nick` chat. */
  pm?: { room: string; nick: string } | null;
  /** The bridge has no presence for this chat yet. The row shows no presence. */
  unknownPresence?: boolean;
  /** A group chat: the people in it now. */
  members?: number | null;
  /** The category of a channel in a space. Channels with the same one share a header. */
  category?: string | null;
}

/** A room outside a space is a group chat. It sits with the DMs, as on Discord. */
export function isGroup(c: ChannelItem): boolean {
  return c.kind === 'channel' && c.space === null && !c.pm;
}

/** "3 Members" under the name of a group chat. Empty when the count is not known. */
export function memberLine(n: number | null | undefined): string {
  return n ? `${n} ${n === 1 ? 'Member' : 'Members'}` : '';
}

export interface MemberItem {
  id: string;
  name: string;
  role: string | null;
  affiliation: Affiliation;
  show: Show;
  /** The presence status text, if the person set one. */
  status: string | null;
  online: boolean;
  avatar: string | null;
  /** Live data: the real JID, if the room shows it. */
  jid?: string | null;
  /** Live data: the nick in the room. */
  nick?: string;
}

export interface Reaction {
  emoji: string;
  count: number;
  mine: boolean;
}

export interface ReplyPreview {
  id: string;
  senderName: string;
  body: string;
}

export interface Attachment {
  url: string;
  name: string;
  mime: string;
  size: number;
  width: number | null;
  height: number | null;
}

export interface TimelineItem {
  id: string;
  /** Live data: the id that the server or the room gave. */
  stanzaId?: string | null;
  originId?: string | null;
  sender: string;
  senderName: string;
  avatar: string | null;
  body: string;
  timestamp: number; // ms since epoch
  outgoing: boolean;
  sameSenderAsPrevious: boolean;
  edited: boolean;
  retracted: boolean;
  reactions: Reaction[];
  replyTo: ReplyPreview | null;
  attachment: Attachment | null;
  status: MessageStatus;
  mention: boolean; // UI: the message mentions me
}

export interface Me {
  address: string;
  name: string;
  avatar: string | null;
  show: Show;
  /** Our status text, or null. */
  status: string | null;
}

export interface PublicCircle {
  service: string;
  node: string;
  name: string;
  description: string;
  /** Null when the server does not tell. */
  members: number | null;
}

export type NotificationLevel = 'all' | 'mentions' | 'nothing';

export function spaceKey(s: Pick<SpaceItem, 'service' | 'node'>): string {
  return `${s.service}/${s.node}`;
}

export function presenceKind(online: boolean, show: Show): PresenceKind {
  if (!online || show === 'invisible') return 'offline';
  if (show === 'dnd') return 'dnd';
  if (show === 'away' || show === 'xa') return 'away';
  return 'online';
}

export const presenceLabel: Record<PresenceKind, string> = {
  online: 'Available',
  away: 'Away',
  dnd: 'Do not disturb',
  offline: 'Offline'
};

/** The label of our own availability. Invisible shows as offline, with its own name. */
export function ownLabel(show: Show): string {
  return show === 'invisible' ? 'Invisible' : presenceLabel[presenceKind(true, show)];
}

export interface FolderCircle {
  id: string;
  name: string;
  avatar: string | null;
}

/** A contact, a pending request, or a blocked address. */
export interface ContactItem {
  address: string;
  name: string;
  avatar: string | null;
  show: Show;
  online: boolean;
  /** Free status text the contact set, if any. */
  status: string | null;
  /** Ms since epoch. Null when unknown. */
  since: number | null;
  /** When the contact went idle, as an xs:dateTime (XEP-0319). */
  idleSince?: string | null;
  /** The song that the contact plays now, as "Artist - Title" (XEP-0118). */
  activity?: string | null;
}

/** Everything the profile views need about one address. */
export interface Person {
  address: string;
  name: string;
  avatar: string | null;
  show: Show;
  online: boolean;
  status: string | null;
  since: number | null;
  isMe: boolean;
  isContact: boolean;
  isBlocked: boolean;
  /** Set when the person is a member of the space you are in. */
  affiliation: Affiliation | null;
  role: string | null;
}

export type ContactsTab = 'online' | 'all' | 'pending' | 'blocked' | 'add';

export type SettingsPage =
  | 'account'
  | 'privacy'
  | 'notifications'
  | 'behaviour'
  | 'appearance'
  | 'chat'
  | 'keybinds'
  | 'advanced'
  | 'about';

export type DisplayMode = 'cozy' | 'compact';

export function affiliationLabel(a: Affiliation): string {
  return a === 'none' ? 'Guest' : a[0].toUpperCase() + a.slice(1);
}
