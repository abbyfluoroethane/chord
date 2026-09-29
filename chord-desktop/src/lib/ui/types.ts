// Local view types. They mirror the core view items and get swapped for the
// bridge types later. Fields marked "UI" are extra and the bridge may derive them.

export type Show = 'chat' | 'away' | 'xa' | 'dnd' | null;
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
}

export interface MemberItem {
  id: string;
  name: string;
  role: string | null;
  affiliation: Affiliation;
  show: Show;
  online: boolean;
  avatar: string | null;
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
}

export interface PublicCircle {
  service: string;
  node: string;
  name: string;
  description: string;
  members: number;
}

export type NotificationLevel = 'all' | 'mentions' | 'nothing';

export function spaceKey(s: Pick<SpaceItem, 'service' | 'node'>): string {
  return `${s.service}/${s.node}`;
}

export function presenceKind(online: boolean, show: Show): PresenceKind {
  if (!online) return 'offline';
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

export interface FolderCircle {
  id: string;
  name: string;
  avatar: string | null;
}
