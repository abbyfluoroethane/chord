// TypeScript types for the JSON that the Rust side sends and receives.
//
// THESE TYPES MUST MATCH chord-core's serde output. They are written by hand. When you
// change a public type in chord-core (feature "serde"), change it here. The test
// chord-core/tests/serde_json.rs pins the field names. Rules of the serde output:
//   - Fields are camelCase. A Rust Option is `T | null` (never missing).
//   - A unit enum is a camelCase string: "displayed", "stanzaId".
//   - An enum with data is `{ type: "name", ... }`:
//       ChannelScope and ChannelKind put the fields next to `type` (internal tag).
//       ListDiff puts them next to `type` too (see ListDiff below).
//       ClientEvent, ConnectionState, ConnectError and AuthFailure put the data in
//       `data` (adjacent tag). A variant with no data has no `data`.
//   - A JID is a string. A Rust tuple is an array.

/** A bare JID (`user@host`) or a full JID (`user@host/resource`), as a string. */
export type Jid = string;

// ---------------------------------------------------------------- errors

/** The error of a rejected command (Rust `ChordError`). */
export interface ChordError {
  code: ErrorCode;
  message: string;
}

export type ErrorCode =
  | 'notOpen' // call `open` first
  | 'notConnected' // no session: call `login`
  | 'session'
  | 'server'
  | 'invalid' // a bad argument, for example a JID
  | 'unsupported' // the server lacks the service
  | 'actorGone'
  | 'authFailed' // login: wrong credentials
  | 'unreachable' // login: no connection
  | 'tlsInvalid' // login: bad server certificate
  | 'timeout' // login: no answer
  | 'store' // the database
  | 'keychain'
  | 'settings'
  | 'io';

// ---------------------------------------------------------------- views

/** A change to a list. Indexes refer to the list after all earlier diffs of the batch. */
export type ListDiff<T> =
  | { type: 'insert'; index: number; item: T }
  | { type: 'update'; index: number; item: T }
  | { type: 'remove'; index: number }
  /** The whole list. The first diff of each subscription. */
  | { type: 'reset'; items: T[] };

export type ChannelScope =
  | { type: 'home' }
  | { type: 'space'; service: string; node: string };

export type ChannelKind =
  | { type: 'direct' }
  | { type: 'room' }
  /** The `jid` of the item is `room/nick`. */
  | { type: 'privateMessage'; room: string; nick: string };

export interface ChannelItem {
  /** Bare JID of the room or the peer. `room/nick` for a private message channel. */
  jid: string;
  name: string;
  kind: ChannelKind;
  category: string | null;
  joined: boolean;
  /** Unix ms. */
  lastActivity: number | null;
  unread: number;
}

export interface SpaceItem {
  /** Pubsub service JID. */
  service: string;
  node: string;
  name: string;
  /** Avatar hash. Use `avatarUrl(hash)`. */
  avatar: string | null;
}

export interface MemberItem {
  id: string;
  name: string;
  jid: string | null;
  /** "moderator", "participant" or "visitor". */
  role: string;
  /** "owner", "admin", "member" or "none". */
  affiliation: string;
  /** "away", "chat", "dnd" or "xa". null means available or offline. */
  show: string | null;
  online: boolean;
  avatar: string | null;
}

export type DeliveryStatus = 'sent' | 'received' | 'displayed';

export interface ReactionSummary {
  emoji: string;
  count: number;
  mine: boolean;
}

export interface ReplyPreview {
  id: string | null;
  senderName: string;
  body: string;
}

export interface TimelineItem {
  /** Stable id `m:<row>`. Commands that name a message take it. */
  id: string;
  stanzaId: string | null;
  originId: string | null;
  sender: string;
  senderName: string;
  avatar: string | null;
  body: string;
  /** Unix ms. */
  timestamp: number;
  outgoing: boolean;
  sameSenderAsPrevious: boolean;
  edited: boolean;
  retracted: boolean;
  reactions: ReactionSummary[];
  replyTo: ReplyPreview | null;
  attachment: string | null;
  status: DeliveryStatus;
}

// ---------------------------------------------------------------- events

export type SaslCondition = string; // the Rust name, for example "NotAuthorized"

export type AuthFailure =
  | { type: 'sasl'; data: SaslCondition }
  | { type: 'noMechanism' }
  | { type: 'local'; data: string };

export type ConnectError =
  | { type: 'authFailed'; data: AuthFailure }
  | { type: 'unreachable'; data: string }
  | { type: 'tlsInvalid'; data: string }
  | { type: 'timeout' };

export type ConnectionState =
  | { type: 'connecting' }
  | { type: 'connected'; data: { boundJid: Jid; resumed: boolean } }
  | { type: 'suspended' }
  | { type: 'loginFailed'; data: ConnectError }
  | { type: 'authFailed'; data: AuthFailure }
  | { type: 'disconnected' };

export type MessageKind = 'chat' | 'groupchat';
export type KeyKind = 'stanzaId' | 'originId';
export type Direction = 'in' | 'out';

export interface StoredMessage {
  rowid: number;
  kind: MessageKind;
  keyKind: KeyKind;
  key: string;
  direction: Direction;
  peer: string;
  sender: string;
  body: string;
  timestamp: number;
}

export interface Notification {
  peer: string;
  room: string | null;
  sender: string;
  senderName: string;
  bodyPreview: string;
  mention: boolean;
  itemId: string;
}

export type ClientEvent =
  | { type: 'connectionState'; data: ConnectionState }
  | { type: 'messageReceived'; data: StoredMessage }
  | { type: 'notice'; data: string }
  | { type: 'subscriptionRequest'; data: Jid }
  | { type: 'notification'; data: Notification }
  | {
      type: 'roomInvite';
      data: { room: Jid; from: Jid; reason: string | null; password: string | null };
    }
  /** `typers` holds bare JIDs in a chat and nicks in a room. Empty: nobody types. */
  | { type: 'typing'; data: { peer: string; typers: string[] } };

// ---------------------------------------------------------------- values

export type NotificationLevel = 'all' | 'mentions' | 'none';

export interface NotificationSetting {
  level: NotificationLevel;
  /** Unix ms. Until then the peer does not notify. */
  muteUntil: number | null;
}

export type RoomAffiliation = 'owner' | 'admin' | 'member' | 'none' | 'outcast';

/** A null or missing field stays as it is. */
export interface RoomSettings {
  name?: string | null;
  public?: boolean | null;
  membersOnly?: boolean | null;
}

export type SpaceAccess = 'open' | 'authorize' | 'whitelist';
export type JoinOutcome = 'joined' | 'pending';

export interface SpaceInfo {
  service: string;
  node: string;
  name: string;
  description: string | null;
  accessModel: string | null;
}

export interface JoinRequest {
  jid: string;
  subid: string | null;
}

/** service, node, name. */
export type PendingJoin = [service: string, node: string, name: string];

export type Subscription = 'none' | 'to' | 'from' | 'both';

export interface Contact {
  jid: Jid;
  name: string | null;
  subscription: Subscription;
  ask: boolean;
  groups: string[];
  approved: boolean;
}

export interface PushRegistration {
  service: string;
  node: string;
}

// ---------------------------------------------------------------- bridge

/** What `open` returns (Rust `OpenInfo`). */
export interface OpenInfo {
  account: Jid;
  hasSavedPassword: boolean;
}

/**
 * The local settings. The UI owns the shape, Rust only stores the JSON (256 KB at most).
 * Keep the fields you know and pass unknown fields back, so that a newer version works.
 */
export type Settings = Record<string, unknown>;
