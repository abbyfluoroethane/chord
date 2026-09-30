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
  | 'notAuthorized' // a room needs a password, or the password is wrong
  | 'conflict' // the nick is in use in the room
  | 'registrationRequired' // only members can join the room
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
  /** Direct chats only: the blocklist holds the peer. */
  blocked: boolean;
  /** A joined room outside a space: the people in it now. */
  members: number | null;
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

export type DeliveryStatus = 'sent' | 'received' | 'displayed' | 'failed';

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

/** XEP-0446 file metadata. */
export interface FileInfo {
  name: string | null;
  size: number | null;
  mediaType: string | null;
  /** SHA-256 of the file, base64. */
  sha256: string | null;
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
  /** XEP-0446: what the sender said about the file. Every part can be missing. */
  attachmentInfo?: FileInfo | null;
  status: DeliveryStatus;
}

/** One stored message that matches a search. `id` is the timeline id. */
export interface SearchHit {
  id: string;
  kind: 'chat' | 'groupchat';
  direction: 'in' | 'out';
  peer: string;
  sender: string;
  body: string;
  /** Unix ms. */
  timestamp: number;
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
  /** The owner destroyed a room that we were in. The core left it and retracted its bookmark. */
  | { type: 'roomDestroyed'; data: { room: Jid; reason: string | null; alternate: Jid | null } }
  /** `typers` holds bare JIDs in a chat and nicks in a room. Empty: nobody types. */
  | { type: 'typing'; data: { peer: string; typers: string[] } }
  /** The blocklist changed. Read it again with `blockedContacts`. */
  | { type: 'blockListChanged' }
  | { type: 'contactChanged'; data: Jid };

// ---------------------------------------------------------------- values

export type NotificationLevel = 'all' | 'mentions' | 'none';

export interface NotificationSetting {
  level: NotificationLevel;
  /** Unix ms. Until then the peer does not notify. */
  muteUntil: number | null;
}

export type RoomAffiliation = 'owner' | 'admin' | 'member' | 'none' | 'outcast';

/** The role of an occupant. `none` kicks, `visitor` mutes, `participant` gives voice back. */
export type RoomRole = 'none' | 'visitor' | 'participant' | 'moderator';

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

/** What `spaceInfo` reads about a space. `channels` is null when the service hides the items. */
export interface SpaceCard {
  service: string;
  node: string;
  name: string;
  description: string | null;
  accessModel: string | null;
  channels: number | null;
}

/** What `roomInfo` reads about a room. */
export interface RoomCard {
  jid: string;
  name: string | null;
  description: string | null;
  subject: string | null;
  occupants: number | null;
  passwordProtected: boolean;
  membersOnly: boolean;
  /** Any occupant may change the subject. False: only moderators, or the room does not say. */
  changeSubject: boolean;
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
  /** The blocklist (XEP-0191) holds this contact. */
  blocked: boolean;
  /** At least one resource of the contact is available. */
  online: boolean;
  /** The show value of the best resource: away, chat, dnd or xa. */
  show: string | null;
  /** The status text of the best resource. */
  status: string | null;
  /** When the user of the best resource stopped interacting, as an xs:dateTime (XEP-0319). */
  idleSince: string | null;
  /** The song that the contact plays now, as "Artist - Title" (XEP-0118). */
  activity: string | null;
}

/** How the server can hide us: the invisible command (XEP-0186) or a privacy list (XEP-0016). */
export type InvisibleMethod = 'command' | 'privacyList';

/** Our availability. Matches chord-core presence::Availability. */
export type Availability = 'available' | 'away' | 'dnd' | 'extendedAway' | 'invisible';

/** What an account tells about itself: the nickname (XEP-0172) and the vCard4 name (XEP-0292). */
export interface Profile {
  nickname: string | null;
  fullName: string | null;
}

export interface OwnPresence {
  availability: Availability;
  status: string | null;
}

export interface PushRegistration {
  service: string;
  node: string;
}

/** A pinned message (Rust `Pin`). The account keeps the pins, so every device shows them. */
export interface Pin {
  /** The bare JID of the room or the contact. */
  chat: string;
  /** The stanza-id, origin-id, or id of the message. Use it with `unpinMessage`. */
  key: string;
  /** The id of the message in the timeline, if this device has the message. */
  itemId: string | null;
  /** The nick in a room, else the JID of the sender. */
  sender: string;
  body: string;
  /** The time of the message, ms. */
  timestamp: number;
  /** The time of the pin, ms. */
  pinnedAt: number;
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

/** The preview of a link (Rust `LinkPreview`). Every field but `url` can be null. */
export interface LinkPreview {
  /** The URL after the redirects. */
  url: string;
  siteName: string | null;
  title: string | null;
  description: string | null;
  /** An absolute http or https URL. */
  image: string | null;
  imageWidth: number | null;
  imageHeight: number | null;
}

/** One file of a GIF. Matches gif.rs GifFile. */
export interface GifFile {
  url: string;
  width: number;
  height: number;
}

/** One GIF from the KLIPY search. Matches gif.rs Gif. */
export interface Gif {
  slug: string;
  title: string;
  /** A small file for the picker grid. */
  preview: GifFile;
  /** The file to send. */
  full: GifFile;
}

/** One page of GIF results. Matches gif.rs GifPage. */
export interface GifPage {
  items: Gif[];
  hasNext: boolean;
}

/** One emoji pack on this computer. Matches emoji.rs PackStatus. */
export interface EmojiPackStatus {
  id: string;
  installed: boolean;
  /** The pack ships with the app and needs no download. */
  bundled: boolean;
}

// ---------------------------------------------------------------- data forms (XEP-0004)

/** The type of a field. The names are the ones in the XEP. */
export type FieldKind =
  | 'boolean'
  | 'fixed'
  | 'hidden'
  | 'jid-multi'
  | 'jid-single'
  | 'list-multi'
  | 'list-single'
  | 'text-multi'
  | 'text-private'
  | 'text-single';

export type FormKind = 'form' | 'submit' | 'cancel' | 'result';

export interface FormOption {
  label: string | null;
  value: string;
}

/** A media element of a field, for example a CAPTCHA image. A `cid:` image is a `data:` URI. */
export interface FormMedia {
  uri: string;
  mime: string | null;
  width: number | null;
  height: number | null;
}

export interface FormField {
  /** Only a `fixed` field has no name. */
  var: string | null;
  kind: FieldKind;
  label: string | null;
  desc: string | null;
  required: boolean;
  values: string[];
  options: FormOption[];
  media: FormMedia[];
}

/** A data form. The UI changes the `values` and sends the same object back. */
export interface DataForm {
  kind: FormKind;
  title: string | null;
  instructions: string | null;
  fields: FormField[];
}

// ---------------------------------------------------------------- ad-hoc commands (XEP-0050)

export interface CommandItem {
  jid: string;
  node: string;
  name: string | null;
}

export type CommandAction = 'execute' | 'next' | 'prev' | 'complete' | 'cancel';
export type CommandStatus = 'executing' | 'completed' | 'canceled';

export interface CommandNote {
  /** "info", "warn" or "error". */
  kind: string;
  text: string;
}

export interface CommandStep {
  node: string;
  /** Give it back with the next step. */
  sessionId: string | null;
  status: CommandStatus;
  actions: CommandAction[];
  defaultAction: CommandAction | null;
  notes: CommandNote[];
  form: DataForm | null;
}

// ---------------------------------------------------------------- registration (XEP-0077)

export interface OobLink {
  url: string;
  desc: string | null;
}

/** What a server wants for a registration. */
export interface RegistrationForm {
  instructions: string | null;
  /** The data form. When it is set, answer with it. */
  form: DataForm | null;
  /** The names of the legacy fields. Used when there is no data form. */
  fields: string[];
  /** A web page for the registration. */
  oob: OobLink | null;
  registered: boolean;
}

/** The answer: the filled data form, or the legacy fields as [name, value] pairs. */
export type RegistrationSubmission = { form: DataForm } | { fields: [string, string][] };
