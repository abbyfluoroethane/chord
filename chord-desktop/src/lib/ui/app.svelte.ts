// UI state. In the browser preview it runs on sample data. Inside Tauri the live
// controller (live.svelte.ts) fills the lists from the bridge, and the actions
// below call the bridge. Each action names its bridge call.
import * as fx from '$lib/fixtures/data';
import type {
  Availability,
  ChordError,
  Gif,
  RoomRole,
  NotificationSetting,
  SpaceAccess,
  TimelineSubscription
} from '$lib/chord';
import { levelToBridge, plainError, splitPrivate, splitSpaceKey, toPublicCircle } from './adapt';
import { api, live } from './bridge';
import { pasteProblem } from './filetransfer';
import { linkPreviews } from './linkpreviews.svelte';
import { settings } from './local';
import { bumpReaction, topReactions, type ReactionUse } from './reactions';
import type {
  ChannelItem,
  MemberItem,
  NotificationLevel,
  PublicCircle,
  Show,
  SpaceItem,
  TimelineItem
} from './types';
import { failureNote, runBatch } from './batch';
import { canSetTopic, isModerator } from './rooms';
import { roomIsMissing, sharePasswordQuestion } from './roomjoin';
import { isGroup, spaceKey } from './types';
import { totalUnread as sumUnread, unreadChannels } from './unread';
import { ui } from './ui.svelte';

export const HOME = 'home';
const HIDDEN_KEY = 'chord.hiddenDms';
const REACTIONS_KEY = 'chord.reactionUse';

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

/** What the UI waits for before it selects something: a new space or channel. */
interface Pending {
  space?: string;
  jid?: string;
}

const slug = (s: string) =>
  s
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');

class AppState {
  me = $state(
    live
      ? { address: '', name: '', avatar: null, show: 'chat' as Show, status: null as string | null }
      : clone(fx.me)
  );
  spaces = $state<SpaceItem[]>(live ? [] : clone(fx.spaces));
  channels = $state<ChannelItem[]>(live ? [] : clone(fx.channels));
  timelines = $state<Record<string, TimelineItem[]>>(live ? {} : clone(fx.timelines));
  members = $state<Record<string, MemberItem[]>>(live ? {} : clone(fx.members));
  typing = $state<Record<string, string[]>>(live ? {} : clone(fx.typing));
  publicCircles = $state<PublicCircle[]>(live ? [] : fx.publicCircles);
  /** Spaces that wait for the owner to approve us (live). */
  pendingJoins = $state<{ service: string; node: string; name: string }[]>([]);

  /** The space list arrived (always true for sample data). */
  spacesReady = $state(!live);

  /** First unread message id per channel. Drives the "new" divider. */
  newFrom = $state<Record<string, string | null>>(live ? {} : { ...fx.firstUnread });
  notifyLevel = $state<Record<string, NotificationLevel>>({});
  nickname = $state<Record<string, string>>({});

  // Live data that the controller keeps.
  /** Notification level of each peer, from the bridge. */
  levels = $state<Record<string, NotificationSetting>>({});
  /** Mentions since the channel was last read, from Notification events. */
  mentions = $state<Record<string, number>>({});
  /** Ids of the messages that mention us. */
  mentionIds = $state<Record<string, true>>({});
  /** Presence of the person in the open direct chat. */
  dmPresence = $state<Record<string, { show: Show; online: boolean }>>({});
  /** Chats that exist only here, until the first message. */
  localChannels = $state<ChannelItem[]>([]);
  /** The running timeline subscription of the open channel (live). */
  timelineSub: TimelineSubscription | null = null;
  private pending: Pending | null = null;

  selectedSpace = $state<string>(HOME);
  private lastChannel: Record<string, string> = {};
  selectedJid = $state<string>(live ? '' : 'launch-ops-general@chat.foid.space');

  replyingTo = $state<TimelineItem | null>(null);
  editingId = $state<string | null>(null);

  /** Home shows the contacts page instead of a DM. */
  showContacts = $state(live);
  /** A file is being dragged over the window. The chat shows a drop hint. */
  dropping = $state(false);
  /** DMs the user closed. They come back when the user opens them again. Local only. */
  hiddenDms = $state<string[]>([]);

  /** How often this user sent each emoji as a reaction. Local to this device. */
  reactionUse = $state<ReactionUse>({});
  /** The four emoji of the quick row in the message menu. */
  quickReactions = $derived(topReactions(this.reactionUse));

  /** How many unread messages the open channel had when the user opened it. */
  private unreadOnOpen: Record<string, number> = {};
  private typingTo: string | null = null;
  private typingTimer: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    if (live) return;
    // Sample data: start in the first space, on its first channel.
    this.selectedSpace = spaceKey(this.spaces[0]);
    this.enterChannel();
  }

  // --- derived -----------------------------------------------------

  channel = $derived(this.channels.find((c) => c.jid === this.selectedJid) ?? null);
  items = $derived.by(() => {
    const list = this.timelines[this.selectedJid] ?? [];
    if (!live) return list;
    return list.map((m) => (this.mentionIds[m.id] ? { ...m, mention: true } : m));
  });
  dividerId = $derived(this.newFrom[this.selectedJid] ?? null);
  // A group chat on the home list keeps its members under HOME (live.svelte.ts watchMembers).
  membersHere = $derived(
    this.selectedSpace === HOME && !(this.channel && isGroup(this.channel))
      ? []
      : (this.members[this.selectedSpace] ?? [])
  );
  /**
   * What the right rail shows for the open chat: the members of a channel or a group chat,
   * the profile of the peer of a 1:1 DM, or nothing (a private message in a room).
   */
  sideRail = $derived.by((): 'members' | 'profile' | null => {
    const c = this.channel;
    if (this.selectedSpace !== HOME) return 'members';
    if (this.showContacts || !c) return null;
    if (isGroup(c)) return 'members';
    return c.kind === 'dm' && !c.pm ? 'profile' : null;
  });
  spaceChannels = $derived(this.channelsOf(this.selectedSpace));
  currentSpace = $derived(this.spaces.find((s) => spaceKey(s) === this.selectedSpace) ?? null);
  typingHere = $derived(this.typing[this.selectedJid] ?? []);
  /** I can delete the messages of other people in this room. */
  canModerate = $derived.by(() => {
    const mine = this.membersHere.find((m) => m.id === this.me.address);
    return !!mine && (mine.affiliation === 'owner' || mine.affiliation === 'admin' || mine.role === 'Moderator');
  });
  /** What the rooms told us: may any occupant change the topic. */
  subjectOpen = $state<Record<string, boolean>>({});
  /** I can set the topic of the open room. */
  canSetTopic = $derived.by(() => {
    if (this.channel?.kind !== 'channel') return false;
    const mine = this.membersHere.find((m) => m.id === this.me.address);
    return canSetTopic(mine, this.subjectOpen[this.selectedJid] ?? false);
  });
  /** I am a moderator of the open room: I can kick and mute. */
  isModerator = $derived(
    isModerator(this.membersHere.find((m) => m.id === this.me.address))
  );
  /** I can change the roles and the settings of this room. */
  isRoomAdmin = $derived.by(() => {
    const mine = this.membersHere.find((m) => m.id === this.me.address);
    return !!mine && (mine.affiliation === 'owner' || mine.affiliation === 'admin');
  });

  /** Home: the chats and the rooms that are in no space. */
  private channelsOf(key: string): ChannelItem[] {
    return key === HOME
      ? this.channels.filter((c) => c.space === null && !this.hiddenDms.includes(c.jid))
      : this.channels.filter((c) => c.space === key);
  }

  spaceOf(key: string): SpaceItem | undefined {
    return this.spaces.find((s) => spaceKey(s) === key);
  }

  /** My nickname in a space. */
  myNick(space: string | null): string {
    return (space && this.nickname[space]) || this.me.name || this.me.address.split('@')[0];
  }

  /** Totals for the rail. Muted channels do not count. */
  spaceBadge(key: string): { unread: number; mentions: number } {
    let unread = 0;
    let mentions = 0;
    for (const c of this.channels) {
      const inSpace = key === HOME ? c.space === null : c.space === key;
      if (!inSpace || c.muted) continue;
      unread += c.unread;
      mentions += c.mentions;
    }
    return { unread, mentions };
  }

  // --- navigation --------------------------------------------------

  selectSpace(key: string) {
    if (key === this.selectedSpace) return;
    this.leaveChannel();
    this.selectedSpace = key;
    this.showContacts = false;
    const list = this.channelsOf(key);
    const remembered = this.lastChannel[key];
    const target = list.find((c) => c.jid === remembered) ?? list[0];
    this.selectedJid = target?.jid ?? '';
    this.enterChannel();
  }

  selectChannel(jid: string) {
    const c = this.channels.find((x) => x.jid === jid);
    if (!c) return;
    this.showContacts = false;
    this.hiddenDms = this.hiddenDms.filter((x) => x !== jid);
    if (jid === this.selectedJid) return;
    this.leaveChannel();
    this.selectedSpace = c.space ?? HOME;
    this.selectedJid = jid;
    this.lastChannel[this.selectedSpace] = jid;
    this.enterChannel();
  }

  /** Go Home and show the contacts page. */
  openContacts() {
    this.selectSpace(HOME);
    this.showContacts = true;
  }

  /** Hide a DM from the list. The messages stay. */
  closeDm(jid: string) {
    if (!this.hiddenDms.includes(jid)) this.hiddenDms.push(jid);
    if (live) settings.set('hiddenDms', $state.snapshot(this.hiddenDms));
    else {
      try {
        localStorage.setItem(HIDDEN_KEY, JSON.stringify(this.hiddenDms));
      } catch {
        /* ignore */
      }
    }
    if (live) void this.sendGone(jid);
    if (jid !== this.selectedJid) return;
    const next = this.spaceChannels[0];
    if (next) this.selectChannel(next.jid);
    else this.showContacts = true;
  }

  loadLocal() {
    linkPreviews.load();
    if (live) {
      this.hiddenDms = settings.get<string[]>('hiddenDms') ?? [];
      this.nickname = settings.get<Record<string, string>>('nicks') ?? {};
      this.reactionUse = settings.get<ReactionUse>('reactionUse') ?? {};
      return;
    }
    try {
      const raw = localStorage.getItem(HIDDEN_KEY);
      if (raw) this.hiddenDms = JSON.parse(raw) as string[];
      const use = localStorage.getItem(REACTIONS_KEY);
      if (use) this.reactionUse = JSON.parse(use) as ReactionUse;
    } catch {
      /* ignore */
    }
  }

  private leaveChannel() {
    this.stopTyping();
    this.replyingTo = null;
    this.editingId = null;
    // Leaving a channel reads it.
    this.newFrom[this.selectedJid] = null;
  }

  private enterChannel() {
    this.lastChannel[this.selectedSpace] = this.selectedJid;
    const c = this.channels.find((x) => x.jid === this.selectedJid);
    if (live) {
      if (!c) return;
      this.unreadOnOpen[c.jid] = c.unread;
      this.mentions[c.jid] = 0;
      void this.openLive(c);
      return;
    }
    if (c) {
      c.unread = 0;
      c.mentions = 0;
    }
  }

  /** Join the room if needed and mark the channel read. */
  private async openLive(c: ChannelItem) {
    try {
      const b = await api();
      if (c.kind === 'channel' && !c.joined) {
        const nick = this.myNick(c.space);
        const typed = await this.joinAsking(b, c.jid, nick);
        // A room in a space is listed by the space. Only a room outside one is bookmarked.
        if (!c.space) {
          await this.bookmark(b, c.jid, nick);
          if (typed !== undefined) this.offerSharedPassword(c.jid, nick);
        }
      }
      if (c.kind === 'channel') void this.readSubjectRight(c.jid);
      await this.readOnBridge(c);
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  /**
   * Join a room. A room that needs a password, or that refuses ours, makes the UI ask the
   * user, and the join runs again with the answer. A cancel rejects with the first error.
   * Maps to api.joinRoom(room, nick, password, fallbackNick).
   */
  private async joinAsking(
    b: Awaited<ReturnType<typeof api>>,
    jid: string,
    nick: string | null,
    password?: string,
    fallbackNick?: string
  ): Promise<string | undefined> {
    let given = password;
    let typed: string | undefined;
    for (;;) {
      try {
        await b.joinRoom(jid, nick, given, fallbackNick);
        // The password that the user typed in the question, if there was one.
        return typed;
      } catch (e) {
        if ((e as ChordError | null)?.code !== 'notAuthorized') throw e;
        const asked = await ui.askPassword(jid, given !== undefined);
        if (asked === null) throw e;
        given = asked;
        typed = asked;
      }
    }
  }

  /**
   * After a join with a password that the user typed: ask if the bookmark should carry it,
   * so that the other devices join too. The bookmark is private to the account, but the
   * server keeps the password as plain text, so the answer is the user's. The password stays
   * on this device (in the system keychain) whatever the answer is.
   * Maps to api.addBookmark(room, nick, name, sharePassword).
   */
  private offerSharedPassword(jid: string, nick: string | null) {
    ui.confirm = {
      title: 'Share the room password?',
      text: sharePasswordQuestion(jid),
      confirm: 'Share the password',
      onconfirm: () => {
        void (async () => {
          try {
            await (await api()).addBookmark(jid, nick, null, true);
            ui.say('The password is in your bookmarks.');
          } catch (e) {
            ui.say(plainError(e));
          }
        })();
      }
    };
  }

  /** Ask once if any occupant may set the topic of a room. A failure keeps "no". */
  private async readSubjectRight(jid: string) {
    if (jid in this.subjectOpen) return;
    try {
      const b = await api();
      this.subjectOpen[jid] = (await b.roomInfo(jid)).changeSubject;
    } catch {
      /* The room says nothing: only moderators set the topic. */
    }
  }

  /** Bookmark a room with autojoin, so that the next login joins it. A failure is not fatal. */
  private async bookmark(b: Awaited<ReturnType<typeof api>>, jid: string, nick: string | null) {
    try {
      await b.addBookmark(jid, nick);
    } catch {
      ui.say('Could not bookmark the channel, so it will not open again at sign-in.', true);
    }
  }

  private async readOnBridge(c: { jid: string; pm?: { room: string; nick: string } | null }) {
    const b = await api();
    if (c.pm) await b.markReadPrivate(c.pm.room, c.pm.nick);
    else await b.markRead(c.jid);
  }

  /** The controller calls this when a list changed: pick something if nothing fits. */
  ensureSelection() {
    this.resolvePending();
    if (this.selectedSpace === HOME && this.showContacts) return;
    const list = this.spaceChannels;
    if (list.some((c) => c.jid === this.selectedJid)) return;
    const next = list.find((c) => c.jid === this.lastChannel[this.selectedSpace]) ?? list[0];
    if (next) {
      this.selectedJid = next.jid;
      this.enterChannel();
    } else if (this.selectedSpace === HOME) {
      this.selectedJid = '';
      this.showContacts = true;
    } else if (this.selectedSpace !== HOME && !this.spaceOf(this.selectedSpace) && this.spacesReady) {
      // The space is gone (left, or deleted).
      this.selectedSpace = HOME;
      this.selectedJid = '';
      this.showContacts = true;
    }
  }

  private resolvePending() {
    const p = this.pending;
    if (!p) return;
    if (p.space) {
      if (!this.spaceOf(p.space)) return;
      if (this.selectedSpace !== p.space) this.selectSpace(p.space);
    }
    if (p.jid) {
      if (!this.channels.some((c) => c.jid === p.jid)) return;
      this.selectChannel(p.jid);
    }
    this.pending = null;
  }

  /** The "new" line: the first of the unread incoming messages, from the first load. */
  markDivider(jid: string, items: TimelineItem[]) {
    const n = this.unreadOnOpen[jid] ?? 0;
    delete this.unreadOnOpen[jid];
    if (n <= 0) return;
    const incoming = items.filter((m) => !m.outgoing);
    const first = incoming[Math.max(0, incoming.length - n)];
    if (first) this.newFrom[jid] = first.id;
  }

  /** Step through the channels of the current space. */
  step(dir: 1 | -1) {
    const list = this.spaceChannels;
    if (!list.length) return;
    const i = list.findIndex((c) => c.jid === this.selectedJid);
    const next = list[(i + dir + list.length) % list.length];
    this.selectChannel(next.jid);
  }

  /** Step to the next channel with unread messages. Looks at all spaces. */
  stepUnread(dir: 1 | -1) {
    const all = this.channels.filter((c) => !c.muted);
    const i = all.findIndex((c) => c.jid === this.selectedJid);
    for (let n = 1; n <= all.length; n++) {
      const c = all[(((i + dir * n) % all.length) + all.length) % all.length];
      if (c.unread > 0 && c.jid !== this.selectedJid) {
        this.selectChannel(c.jid);
        return;
      }
    }
  }

  /** Maps to api.markRead(peer) or api.markReadPrivate(room, nick). */
  markRead(jid: string = this.selectedJid) {
    const c = this.channels.find((x) => x.jid === jid);
    if (c) {
      c.unread = 0;
      c.mentions = 0;
    }
    this.newFrom[jid] = null;
    if (live && jid) {
      this.mentions[jid] = 0;
      const pm = splitPrivate(jid);
      void this.readOnBridge({ jid, pm }).catch((e) => ui.say(plainError(e), true));
    }
  }

  /**
   * Mark a message and the ones after it as unread, and show the "new" line there.
   * Maps to api.markUnread(itemId). The line starts at the first incoming message from
   * there on, because our own messages are never unread.
   */
  markUnread(m: TimelineItem) {
    const jid = this.selectedJid;
    const list = this.timelines[jid] ?? [];
    const at = list.findIndex((x) => x.id === m.id);
    if (at < 0) return;
    const unread = list.slice(at).filter((x) => !x.outgoing && !x.retracted);
    if (!unread.length) {
      ui.say('Nothing from other people after this message.');
      return;
    }
    this.newFrom[jid] = unread[0].id;
    const c = this.channels.find((x) => x.jid === jid);
    if (c) c.unread = unread.length;
    if (live) void this.call((b) => b.markUnread(m.id));
  }

  /** Mark every channel of a space as read. */
  markSpaceRead(key: string) {
    for (const c of this.channels.filter((x) => (key === HOME ? x.space === null : x.space === key))) {
      if (c.unread > 0 || c.mentions > 0) this.markRead(c.jid);
    }
  }

  /** Mark every channel of every space, and every chat, as read. */
  markAllRead() {
    for (const c of unreadChannels(this.channels)) this.markRead(c.jid);
  }

  /** The messages that wait in all chats, for the window title and the dock badge. */
  totalUnread = $derived(sumUnread(this.channels));

  // --- typing ------------------------------------------------------

  /** The composer calls this on input. Maps to api.setTyping(peer, typing). */
  noteTyping(hasText: boolean) {
    if (!live) return;
    const jid = this.selectedJid;
    if (!jid) return;
    if (!hasText) {
      this.stopTyping();
      return;
    }
    if (this.typingTo !== jid) {
      this.stopTyping();
      this.typingTo = jid;
      void this.sendTyping(jid, true);
    }
    clearTimeout(this.typingTimer);
    // Five seconds without input: we stop typing.
    this.typingTimer = setTimeout(() => this.stopTyping(), 5000);
  }

  stopTyping() {
    clearTimeout(this.typingTimer);
    if (!this.typingTo) return;
    const jid = this.typingTo;
    this.typingTo = null;
    void this.sendTyping(jid, false);
  }

  private async sendTyping(jid: string, typing: boolean) {
    try {
      await (await api()).setTyping(jid, typing);
    } catch {
      /* A lost typing hint does no harm. */
    }
  }

  /** The user closed a chat: tell the peer (XEP-0085 `gone`). */
  private async sendGone(jid: string) {
    if (this.typingTo === jid) this.stopTyping();
    try {
      await (await api()).closeChat(jid);
    } catch {
      /* A lost state does no harm. */
    }
  }

  // --- messages ----------------------------------------------------

  private list(jid: string): TimelineItem[] {
    if (!this.timelines[jid]) this.timelines[jid] = [];
    return this.timelines[jid];
  }

  /**
   * Send a message. Maps to api.sendChat(to, body), api.sendPrivate(room, nick, body)
   * for a private chat, and api.reply(itemId, body) when replying.
   * Live: the timeline shows the message when the diff arrives. Returns false on failure.
   */
  send(body: string): boolean | Promise<boolean> {
    const text = body.trim();
    if (!text) return false;
    if (live) return this.sendLive(text);
    this.pushLocal(this.selectedJid, text, this.replyingTo);
    this.replyingTo = null;
    return true;
  }

  /**
   * Try a failed message again: send its text as a new message, then discard the failed
   * row. Maps to api.retractMessage(itemId), which deletes a failed message locally.
   */
  async retry(m: TimelineItem) {
    if (m.status !== 'failed' || !m.body.trim()) return;
    if (!live) {
      m.status = 'sending';
      setTimeout(() => (m.status = 'sent'), 500);
      return;
    }
    // The text goes out as it is. A reply quote is not kept.
    const jid = this.selectedJid;
    const pm = splitPrivate(jid);
    const r = await this.call((b) =>
      pm ? b.sendPrivate(pm.room, pm.nick, m.body) : b.sendChat(jid, m.body)
    );
    if (r.ok) await this.call((b) => b.retractMessage(m.id));
  }

  /** Sample data: add an outgoing message to a chat. The server confirms a moment later. */
  private pushLocal(jid: string, text: string, reply: TimelineItem | null = null) {
    const list = this.list(jid);
    const prev = list[list.length - 1];
    const now = Date.now();
    const id = `local-${now}-${list.length}`;
    list.push({
      id,
      sender: this.me.address,
      senderName: this.me.name,
      avatar: this.me.avatar,
      body: text,
      timestamp: now,
      outgoing: true,
      sameSenderAsPrevious:
        !!prev && prev.sender === this.me.address && !reply && now - prev.timestamp < 7 * 60_000,
      edited: false,
      retracted: false,
      reactions: [],
      replyTo: reply
        ? { id: reply.id, senderName: reply.senderName, body: reply.body }
        : null,
      attachment: null,
      status: 'sending',
      mention: false
    });
    setTimeout(() => {
      const m = this.timelines[jid]?.find((x) => x.id === id);
      if (m) m.status = 'sent';
    }, 500);
  }

  /**
   * Send a copy of a message to another chat: its text, and the address of its file if
   * it has one. Maps to api.sendChat(to, body) or api.sendPrivate(room, nick, body).
   */
  async forward(m: TimelineItem, jid: string): Promise<boolean> {
    const text = [m.body.trim(), m.attachment?.url].filter(Boolean).join('\n');
    if (!text) return false;
    if (!live) {
      this.pushLocal(jid, text);
      return true;
    }
    const pm = splitPrivate(jid);
    const r = await this.call((b) =>
      pm ? b.sendPrivate(pm.room, pm.nick, text) : b.sendChat(jid, text)
    );
    return r.ok;
  }

  private async sendLive(text: string): Promise<boolean> {
    const jid = this.selectedJid;
    if (!jid) return false;
    const reply = this.replyingTo;
    this.replyingTo = null;
    this.stopTyping();
    try {
      const b = await api();
      const pm = splitPrivate(jid);
      if (reply) await b.reply(reply.id, text);
      else if (pm) await b.sendPrivate(pm.room, pm.nick, text);
      else await b.sendChat(jid, text);
      return true;
    } catch (e) {
      ui.say(plainError(e), true);
      return false;
    }
  }

  /**
   * Send a GIF as a link with an embed (XEP-0066), so that clients show it inline. Maps to
   * api.sendLink(to, url). Sample data: the GIF shows as a local attachment.
   */
  async sendGif(gif: Gif): Promise<boolean> {
    const jid = this.selectedJid;
    if (!jid) return false;
    if (!live) {
      this.pushLocal(jid, '');
      const list = this.list(jid);
      list[list.length - 1].attachment = {
        url: gif.full.url,
        name: gif.title || 'GIF',
        mime: 'image/gif',
        size: 0,
        width: gif.full.width || null,
        height: gif.full.height || null
      };
      return true;
    }
    const r = await this.call((b) => b.sendLink(jid, gif.full.url));
    return r.ok;
  }

  /**
   * Attach a file. PREVIEW ONLY: the file stays in the page and nothing is uploaded. The
   * live app never calls this. It uploads through `uploadPicked` and `uploadDropped`, where
   * Rust reads the file. Do not use this for a new feature that needs a real upload.
   */
  sendFile(file: File) {
    const list = this.list(this.selectedJid);
    const now = Date.now();
    const prev = list[list.length - 1];
    const isImage = file.type.startsWith('image/');
    list.push({
      id: `local-${now}-file`,
      sender: this.me.address,
      senderName: this.me.name,
      avatar: this.me.avatar,
      body: '',
      timestamp: now,
      outgoing: true,
      sameSenderAsPrevious: !!prev && prev.sender === this.me.address && now - prev.timestamp < 7 * 60_000,
      edited: false,
      retracted: false,
      reactions: [],
      replyTo: null,
      attachment: {
        url: URL.createObjectURL(file),
        name: file.name,
        mime: file.type || 'application/octet-stream',
        size: file.size,
        width: isImage ? 360 : null,
        height: isImage ? 240 : null
      },
      status: 'sent',
      mention: false
    });
  }

  /** Let the user pick files and upload them to the open chat. Maps to api.uploadFiles(to). */
  async uploadPicked() {
    const jid = this.selectedJid;
    if (!live || !jid) return;
    try {
      const urls = await (await api()).uploadFiles(jid);
      if (urls.length) ui.say(urls.length === 1 ? 'File sent.' : 'Files sent.');
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  /** Upload a file that the user dropped on the window. Maps to api.uploadDropped(to, path). */
  async uploadDropped(path: string) {
    const jid = this.selectedJid;
    if (!live || !jid) return;
    try {
      await (await api()).uploadDropped(jid, path);
      ui.say('File sent.');
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  /** Upload a file that the user pasted: the page has its bytes and no path. Maps to api.uploadPasted(to, type, bytes). */
  async uploadPasted(file: File) {
    const jid = this.selectedJid;
    if (!live || !jid) return;
    const problem = pasteProblem(file);
    if (problem) {
      ui.say(problem);
      return;
    }
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      await (await api()).uploadPasted(jid, file.type, bytes);
      ui.say('File sent.');
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  /** Maps to api.editMessage(itemId, body). */
  edit(id: string, body: string) {
    const m = this.items.find((x) => x.id === id);
    const text = body.trim();
    if (!m || !text) return;
    if (text !== m.body) {
      if (live) {
        void this.call(async (b) => b.editMessage(id, text));
      } else {
        const own = this.timelines[this.selectedJid]?.find((x) => x.id === id);
        if (own) {
          own.body = text;
          own.edited = true;
        }
      }
    }
    this.editingId = null;
  }

  /** Maps to api.retractMessage(itemId). */
  retract(id: string) {
    if (live) {
      void this.call(async (b) => b.retractMessage(id));
      return;
    }
    const m = this.timelines[this.selectedJid]?.find((x) => x.id === id);
    if (m) {
      m.retracted = true;
      m.attachment = null;
      m.reactions = [];
    }
  }

  /** Delete my message, or as a moderator the message of another. Maps to api.moderateMessage. */
  deleteMessage(m: TimelineItem) {
    if (m.outgoing || !live) {
      this.retract(m.id);
      return;
    }
    void this.call(async (b) => b.moderateMessage(m.id));
  }

  /** Maps to api.toggleReaction(itemId, emoji). */
  toggleReaction(id: string, emoji: string) {
    // Count the reactions that we add. A removed one does not count.
    const shown = (this.timelines[this.selectedJid] ?? []).find((x) => x.id === id);
    if (!shown?.reactions.some((r) => r.emoji === emoji && r.mine)) this.countEmoji(emoji);
    if (live) {
      void this.call(async (b) => b.toggleReaction(id, emoji));
      return;
    }
    const m = this.timelines[this.selectedJid]?.find((x) => x.id === id);
    if (!m || m.retracted) return;
    const r = m.reactions.find((x) => x.emoji === emoji);
    if (!r) {
      m.reactions.push({ emoji, count: 1, mine: true });
    } else if (r.mine) {
      r.count -= 1;
      r.mine = false;
      if (r.count <= 0) m.reactions = m.reactions.filter((x) => x.emoji !== emoji);
    } else {
      r.count += 1;
      r.mine = true;
    }
  }

  /** Count one use of an emoji, for the quick reactions and "Frequently used". */
  countEmoji(emoji: string) {
    this.reactionUse = bumpReaction(this.reactionUse, emoji);
    if (live) {
      settings.set('reactionUse', $state.snapshot(this.reactionUse));
      return;
    }
    try {
      localStorage.setItem(REACTIONS_KEY, JSON.stringify(this.reactionUse));
    } catch {
      /* ignore */
    }
  }

  /** Show older messages. Maps to api.timelinePaginateBack(id, count) via the subscription. */
  async paginateBack(count = 30): Promise<void> {
    try {
      await this.timelineSub?.paginateBack(count);
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  lastOwn(): TimelineItem | null {
    for (let i = this.items.length - 1; i >= 0; i--) {
      const m = this.items[i];
      if (m.outgoing && !m.retracted) return m;
    }
    return null;
  }

  /** Only the last own message of a chat can change (XEP-0308, section 3). */
  canEdit(m: TimelineItem): boolean {
    return !!m.body && this.lastOwn()?.id === m.id;
  }

  startReply(m: TimelineItem) {
    this.editingId = null;
    this.replyingTo = m;
  }

  /** Run a bridge call. A failure shows as a notice in plain words. */
  async call<T>(
    f: (b: Awaited<ReturnType<typeof api>>) => Promise<T>
  ): Promise<{ ok: true; value: T } | { ok: false }> {
    try {
      return { ok: true, value: await f(await api()) };
    } catch (e) {
      ui.say(plainError(e), true);
      return { ok: false };
    }
  }

  // --- notification levels ------------------------------------------

  /** Maps to api.setNotificationLevel(peer, level). */
  async setLevel(jid: string, level: NotificationLevel) {
    this.notifyLevel[jid] = level;
    if (!live) {
      const c = this.channels.find((x) => x.jid === jid);
      if (c) c.muted = level === 'nothing';
      return;
    }
    const bridgeLevel = levelToBridge(level);
    const r = await this.call((b) => b.setNotificationLevel(jid, bridgeLevel));
    if (r.ok) this.levels[jid] = { level: bridgeLevel, muteUntil: null };
  }

  /**
   * Mute a chat for a time, in ms, or until the user turns it back on (`null`). Maps to
   * api.setNotificationLevel(peer, level, muteUntil). A mute for a time keeps the level.
   */
  async muteFor(jid: string, ms: number | null) {
    if (ms === null) {
      await this.setLevel(jid, 'nothing');
      return;
    }
    const until = Date.now() + ms;
    if (!live) {
      const c = this.channels.find((x) => x.jid === jid);
      if (c) c.muted = true;
      this.notifyLevel[jid] = 'all';
      setTimeout(() => {
        const row = this.channels.find((x) => x.jid === jid);
        if (row && this.levelOf(jid) !== 'nothing') row.muted = false;
      }, ms);
      return;
    }
    const level = levelToBridge(this.levelOf(jid) === 'nothing' ? 'all' : this.levelOf(jid));
    const r = await this.call((b) => b.setNotificationLevel(jid, level, until));
    if (r.ok) this.levels[jid] = { level, muteUntil: until };
  }

  /** The chat has a mute that ends. */
  isTimedMute(jid: string): boolean {
    const s = this.levels[jid];
    return !!s && s.muteUntil !== null && s.muteUntil > Date.now();
  }

  /** Turn a mute off. */
  async unmute(jid: string) {
    if (!live) {
      const c = this.channels.find((x) => x.jid === jid);
      if (c) c.muted = false;
      this.notifyLevel[jid] = 'all';
      return;
    }
    const r = await this.call((b) => b.setNotificationLevel(jid, 'all'));
    if (r.ok) this.levels[jid] = { level: 'all', muteUntil: null };
  }

  /** The level of a channel or a chat, for the menus. */
  levelOf(jid: string): NotificationLevel {
    const known = this.levels[jid];
    if (known) return known.level === 'none' ? 'nothing' : known.level;
    return this.notifyLevel[jid] ?? 'all';
  }

  /** The level of a space: the level that all its channels share, or "all". */
  async setCircleLevel(space: string, level: NotificationLevel) {
    this.notifyLevel[space] = level;
    if (!live) return;
    for (const c of this.channels.filter((x) => x.space === space && x.kind === 'channel')) {
      await this.setLevel(c.jid, level);
    }
  }

  // --- spaces -----------------------------------------------------

  /** Maps to api.setPresence(availability, status). Offline, the core only stores it. */
  setShow(show: Show) {
    this.me.show = show;
    void this.pushPresence();
  }

  /** Our status text. An empty text clears it. Maps to api.setPresence. */
  setStatus(text: string | null) {
    const clean = text?.trim() ?? '';
    this.me.status = clean ? clean.slice(0, 128) : null;
    void this.pushPresence();
  }

  /** Read our stored availability and status text. Maps to api.ownPresence. */
  async loadPresence() {
    if (!live) return;
    try {
      const own = await (await api()).ownPresence();
      this.me.show = fromAvailability(own.availability);
      this.me.status = own.status;
    } catch {
      /* keep the defaults */
    }
  }

  /** Read our display name from the server (nickname or vCard4 name). Maps to api.profile. */
  async loadProfile() {
    if (!live || !this.me.address) return;
    try {
      const p = await (await api()).profile(this.me.address);
      const name = p.nickname ?? p.fullName;
      if (name) this.me.name = name;
    } catch {
      /* keep the name from the address */
    }
  }

  private async pushPresence() {
    if (!live) return;
    const done = await this.call((b) => b.setPresence(toAvailability(this.me.show), this.me.status));
    // The core can refuse invisible and keep another availability. Show what it keeps.
    if (!done.ok) await this.loadPresence();
  }

  /** Sample data only. */
  createCircle(name: string): string {
    const clean = name.trim();
    const node = clean.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'circle';
    const s: SpaceItem = { service: 'chat.foid.space', node, name: clean, avatar: null };
    return this.addCircle(s, 'Say hello.');
  }

  /** Sample data only. */
  joinCircle(p: PublicCircle): string {
    return this.addCircle(
      { service: p.service, node: p.node, name: p.name, avatar: null },
      p.description
    );
  }

  /**
   * Create a space. Maps to api.createSpace(name, access), then makes a "general"
   * channel. Returns false when it fails.
   */
  async createCircleAsync(name: string, access: SpaceAccess): Promise<boolean> {
    if (!live) {
      this.createCircle(name);
      return true;
    }
    const made = await this.call((b) => b.createSpace(name.trim(), access));
    if (!made.ok) return false;
    const key = spaceKey({ service: made.value[0], node: made.value[1] });
    this.pending = { space: key };
    await this.makeRoom(key, 'general');
    this.ensureSelection();
    return true;
  }

  /** Join a public space. Maps to api.joinSpace(service, node). */
  async joinCircleAsync(p: PublicCircle): Promise<boolean> {
    if (!live) {
      this.joinCircle(p);
      return true;
    }
    const r = await this.call((b) => b.joinSpace(p.service, p.node));
    if (!r.ok) return false;
    if (r.value === 'pending') {
      ui.say(`Request sent. The owner of ${p.name} has to approve it.`);
      await this.loadPendingJoins();
    } else {
      this.pending = { space: spaceKey(p) };
      this.ensureSelection();
    }
    return true;
  }

  /**
   * Join a room by its address, for an xmpp: link. A room that the app knows opens at once.
   * Maps to api.joinRoom(room, nick, password). A room in no space shows in Home.
   */
  async joinRoomLink(jid: string, password: string | null = null): Promise<boolean> {
    if (this.channels.some((c) => c.jid === jid)) {
      this.selectChannel(jid);
      return true;
    }
    if (!live) {
      const info = fx.xmppRooms[jid];
      this.channels.push({
        jid,
        name: info?.name ?? jid.split('@')[0],
        kind: 'channel',
        unread: 0,
        joined: true,
        mentions: 0,
        muted: false,
        topic: info?.subject ?? null,
        space: null,
        avatar: null,
        show: null,
        online: false
      });
      this.selectChannel(jid);
      return true;
    }
    // A first join: no nick of our own, so that the room can give the nick that it reserved
    // for us. Without one, the nick is what we use in the spaces.
    // A join to a room that does not exist makes a new one, and a typo in an address would
    // do it. Read the room first, and ask before a new room is made (XEP-0045, 10.1).
    const may = await this.call((b) => this.mayJoin(b, jid));
    if (!may.ok || !may.value) return false;
    let typed: string | undefined;
    const r = await this.call(async (b) => {
      typed = await this.joinAsking(b, jid, null, password ?? undefined, this.myNick(null));
      await this.bookmark(b, jid, null);
    });
    if (!r.ok) return false;
    if (typed !== undefined) this.offerSharedPassword(jid, null);
    this.pending = { jid };
    this.ensureSelection();
    return true;
  }

  /**
   * Read the disco#info of a room before a join. A room that does not exist makes the UI
   * ask: true means make it. Any other answer, good or bad, lets the join go on.
   * Maps to api.roomInfo(room).
   */
  private async mayJoin(b: Awaited<ReturnType<typeof api>>, jid: string): Promise<boolean> {
    try {
      await b.roomInfo(jid);
      return true;
    } catch (e) {
      if (roomIsMissing(e)) return ui.askCreateRoom(jid);
      return true;
    }
  }

  /**
   * Accept an invitation to a room. The invitation may carry the password. A room that we
   * know already is joined again, with that password. Maps to api.joinRoom.
   */
  async acceptRoomInvite(jid: string, password: string | null): Promise<boolean> {
    const known = this.channels.find((c) => c.jid === jid && c.kind === 'channel');
    if (!live || !known || known.joined) return this.joinRoomLink(jid, password);
    const r = await this.call(async (b) => {
      await this.joinAsking(b, jid, this.myNick(known.space), password ?? undefined);
    });
    if (r.ok) this.selectChannel(jid);
    return r.ok;
  }

  /** The room is gone: it is not joined any more. The rows go when the bookmark is gone. */
  markRoomGone(jid: string) {
    const c = this.channels.find((x) => x.jid === jid);
    if (c) c.joined = false;
  }

  /** Set the topic of a room that we are in. Maps to api.setRoomSubject(room, subject). */
  async setTopic(jid: string, text: string): Promise<boolean> {
    if (!live) {
      const c = this.channels.find((x) => x.jid === jid);
      if (c) c.topic = text.trim() || null;
      return true;
    }
    const r = await this.call((b) => b.setRoomSubject(jid, text.trim()));
    if (r.ok) ui.say(text.trim() ? 'Topic changed.' : 'Topic cleared.');
    return r.ok;
  }

  /**
   * Kick, mute, or give voice back to a person of the open room, by the nick.
   * Maps to api.setRoomRole(room, nick, role, reason).
   */
  async setRole(address: string, role: RoomRole, reason?: string): Promise<void> {
    const room = this.selectedJid;
    const target = this.membersHere.find((m) => m.id === address);
    if (!live || !room || !target?.nick) return;
    const nick = target.nick;
    const r = await this.call((b) => b.setRoomRole(room, nick, role, reason));
    if (!r.ok) return;
    ui.say(
      role === 'none'
        ? `${target.name} was kicked.`
        : role === 'visitor'
          ? `${target.name} is muted.`
          : 'Saved.'
    );
  }

  /** Maps to api.browseSpaces(). */
  async loadPublicCircles() {
    if (!live) return;
    const r = await this.call((b) => b.browseSpaces());
    if (r.ok) this.publicCircles = r.value.map(toPublicCircle);
  }

  /** Maps to api.pendingSpaceJoins(). */
  async loadPendingJoins() {
    if (!live) return;
    const r = await this.call((b) => b.pendingSpaceJoins());
    if (r.ok) this.pendingJoins = r.value.map(([service, node, name]) => ({ service, node, name }));
  }

  private addCircle(s: SpaceItem, topic: string): string {
    const key = spaceKey(s);
    if (!this.spaceOf(key)) {
      this.spaces.push(s);
      this.channels.push(this.newChannel(key, 'general', topic));
      this.members[key] = clone(fx.members[spaceKey(fx.spaces[0])] ?? []);
    }
    this.selectSpace(key);
    return key;
  }

  private newChannel(space: string, name: string, topic: string | null): ChannelItem {
    const [service, node] = [space.split('/')[0], space.split('/')[1]];
    return {
      jid: `${node}-${name}@${service}`,
      name,
      kind: 'channel',
      unread: 0,
      joined: true,
      mentions: 0,
      muted: false,
      topic,
      space,
      avatar: null,
      show: null,
      online: false
    };
  }

  /**
   * Make a room in a space. Maps to api.roomService (where rooms live), api.joinRoom
   * (which makes it), api.configureRoom (its name), and api.addRoomToSpace.
   */
  private async makeRoom(space: string, name: string): Promise<boolean> {
    const clean = slug(name);
    if (!clean) return false;
    const { service, node } = splitSpaceKey(space);
    let room = '';
    const r = await this.call(async (b) => {
      // The space service holds the space. Rooms live on the room service.
      const rooms = await b.roomService();
      if (!rooms) throw { code: 'unsupported', message: 'This server has no channel service.' };
      room = `${node}-${clean}@${rooms}`;
      await b.joinRoom(room, this.myNick(space));
      try {
        await b.configureRoom(room, { name: clean });
      } catch {
        ui.say('The channel is ready, but only an owner can set its name.', true);
      }
      await b.addRoomToSpace(service, node, room, clean);
    });
    if (r.ok) this.pending = { ...(this.pending ?? {}), jid: this.pending?.jid ?? room };
    return r.ok;
  }

  createChannel(space: string, name: string) {
    const clean = slug(name);
    if (!clean) return;
    if (live) {
      this.pending = { space };
      void this.makeRoom(space, name).then(() => this.ensureSelection());
      return;
    }
    const c = this.newChannel(space, clean, null);
    if (!this.channels.some((x) => x.jid === c.jid)) this.channels.push(c);
    this.selectChannel(c.jid);
  }

  /** Maps to api.leaveSpace(service, node) and api.leaveRoom(room) for each room. */
  leaveCircle(key: string) {
    this.leaveChannel();
    if (live) {
      const { service, node } = splitSpaceKey(key);
      const rooms = this.channels.filter((c) => c.space === key && c.kind === 'channel' && c.joined);
      void this.call(async (b) => {
        const { failed } = await runBatch(rooms, (r) => b.leaveRoom(r.jid));
        await b.leaveSpace(service, node);
        const note = failureNote(failed, rooms.length, 'channel');
        if (note) ui.say(note.replace(' failed.', ' could not be left.'), true);
      });
    } else {
      this.spaces = this.spaces.filter((s) => spaceKey(s) !== key);
      this.channels = this.channels.filter((c) => c.space !== key);
    }
    this.selectedSpace = '';
    this.selectSpace(HOME);
  }

  /** Delete a space for everybody. Only its owner can. Maps to api.deleteSpace(service, node). */
  async deleteCircle(key: string): Promise<boolean> {
    if (live) {
      const { service, node } = splitSpaceKey(key);
      const r = await this.call((b) => b.deleteSpace(service, node));
      if (!r.ok) return false;
    } else {
      this.spaces = this.spaces.filter((s) => spaceKey(s) !== key);
      this.channels = this.channels.filter((c) => c.space !== key);
    }
    this.leaveChannel();
    this.selectedSpace = '';
    this.selectSpace(HOME);
    return true;
  }

  /** Take a channel out of a space (owner only). The room itself stays. Maps to
   * api.removeRoomFromSpace(service, node, room). */
  async removeChannelFromCircle(key: string, jid: string): Promise<boolean> {
    if (live) {
      const { service, node } = splitSpaceKey(key);
      const r = await this.call((b) => b.removeRoomFromSpace(service, node, jid));
      if (!r.ok) return false;
    } else {
      this.channels = this.channels.filter((c) => c.jid !== jid);
    }
    if (jid === this.selectedJid) this.ensureSelection();
    return true;
  }

  /** Leave one channel. Maps to api.leaveRoom(room). The row stays in its space. */
  leaveRoom(jid: string) {
    if (jid === this.selectedJid) {
      this.leaveChannel();
      const next = this.spaceChannels.find((c) => c.jid !== jid);
      this.selectedJid = next?.jid ?? '';
      if (next) this.enterChannel();
    }
    if (live) {
      void this.call((b) => b.leaveRoom(jid));
      return;
    }
    const c = this.channels.find((x) => x.jid === jid);
    if (c) c.joined = false;
  }

  /** Change my nickname in every room of the space. Maps to api.changeNick(room, nick). */
  async changeNick(space: string, nick: string): Promise<void> {
    this.nickname[space] = nick;
    if (!live) return;
    settings.set('nicks', $state.snapshot(this.nickname));
    const rooms = this.channels.filter((c) => c.space === space && c.kind === 'channel' && c.joined);
    await this.call(async (b) => {
      for (const r of rooms) await b.changeNick(r.jid, nick);
    });
  }

  /**
   * Invite a person to a space. Maps to api.addSpaceMember(service, node, jid), then
   * api.inviteToRoom(room, jid) for each room of the space that we know.
   */
  async inviteToCircle(space: string, address: string): Promise<boolean> {
    if (!live) return true;
    const { service, node } = splitSpaceKey(space);
    const rooms = this.channels.filter((c) => c.space === space && c.kind === 'channel');
    const r = await this.call(async (b) => {
      await b.addSpaceMember(service, node, address);
      const { failed } = await runBatch(rooms, (room) => b.inviteToRoom(room.jid, address));
      const note = failureNote(failed, rooms.length, 'channel invite');
      if (note) ui.say(note, true);
    });
    return r.ok;
  }

  /**
   * Invite people to a space: make each one a member, then send a message with the join
   * link. Maps to api.addSpaceMember(service, node, jid) and api.sendChat(jid, body).
   * The member step needs owner rights, and a refusal does not stop the message.
   * Returns how many messages went out.
   */
  async sendSpaceInvites(space: string, addresses: string[], link: string): Promise<number> {
    const s = this.spaceOf(space);
    const text = `Join ${s?.name ?? 'my space'} on Chord: ${link}`;
    if (!live) return addresses.length;
    const { service, node } = splitSpaceKey(space);
    let sent = 0;
    for (const address of addresses) {
      const r = await this.call(async (b) => {
        await b.addSpaceMember(service, node, address).catch(() => undefined);
        await b.sendChat(address, text);
      });
      if (r.ok) sent += 1;
    }
    return sent;
  }

  /** Change the role of a person in the open room. Maps to api.setRoomAffiliation(room, jid, affiliation). */
  async setAffiliation(
    address: string,
    affiliation: 'owner' | 'admin' | 'member' | 'none' | 'outcast'
  ): Promise<void> {
    const room = this.selectedJid;
    if (!live || !room) return;
    const r = await this.call((b) => b.setRoomAffiliation(room, address, affiliation));
    if (r.ok) ui.say('Saved.');
  }

  /** Open a DM with a member. Creates the row when it is missing. */
  openDm(id: string, name: string, info?: { avatar: string | null; show: Show; online: boolean }) {
    if (!this.channels.some((c) => c.jid === id)) {
      const m = info ?? Object.values(this.members).flat().find((x) => x.id === id);
      const pm = splitPrivate(id);
      const row: ChannelItem = {
        jid: id,
        name: pm ? `${pm.nick} in ${pm.room.split('@')[0]}` : name,
        kind: 'dm',
        unread: 0,
        joined: true,
        mentions: 0,
        muted: false,
        topic: null,
        space: null,
        avatar: m?.avatar ?? null,
        show: m?.show ?? null,
        online: m?.online ?? false,
        pm,
        unknownPresence: live
      };
      // Live: the controller keeps the row when it rebuilds the list.
      if (live) this.localChannels.push(row);
      this.channels.push(row);
    }
    this.selectChannel(id);
  }
}

export const app = new AppState();

/** Our show value in the UI, as the core availability. */
function toAvailability(show: Show): Availability {
  if (show === 'dnd') return 'dnd';
  if (show === 'away') return 'away';
  if (show === 'xa') return 'extendedAway';
  if (show === 'invisible') return 'invisible';
  return 'available';
}

function fromAvailability(a: Availability): Show {
  if (a === 'dnd') return 'dnd';
  if (a === 'away') return 'away';
  if (a === 'extendedAway') return 'xa';
  if (a === 'invisible') return 'invisible';
  return 'chat';
}
