// UI state over sample data. The bridge replaces the data sources later.
import * as fx from '$lib/fixtures/data';
import type {
  ChannelItem,
  MemberItem,
  NotificationLevel,
  PublicCircle,
  Show,
  SpaceItem,
  TimelineItem
} from './types';
import { spaceKey } from './types';

export const HOME = 'home';

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

class AppState {
  me = $state(clone(fx.me));
  spaces = $state<SpaceItem[]>(clone(fx.spaces));
  channels = $state<ChannelItem[]>(clone(fx.channels));
  timelines = $state<Record<string, TimelineItem[]>>(clone(fx.timelines));
  members = $state<Record<string, MemberItem[]>>(clone(fx.members));
  typing = $state<Record<string, string[]>>(clone(fx.typing));
  publicCircles: PublicCircle[] = fx.publicCircles;

  /** First unread message id per channel. Drives the "new" divider. */
  newFrom = $state<Record<string, string | null>>({ ...fx.firstUnread });
  notifyLevel = $state<Record<string, NotificationLevel>>({});
  nickname = $state<Record<string, string>>({});

  selectedSpace = $state<string>(HOME);
  private lastChannel: Record<string, string> = {};
  selectedJid = $state<string>('launch-ops-general@chat.foid.space');

  replyingTo = $state<TimelineItem | null>(null);
  editingId = $state<string | null>(null);

  constructor() {
    // Start in the first circle, on its first channel.
    this.selectedSpace = spaceKey(this.spaces[0]);
    this.enterChannel();
  }

  // --- derived -----------------------------------------------------

  channel = $derived(this.channels.find((c) => c.jid === this.selectedJid) ?? null);
  items = $derived(this.timelines[this.selectedJid] ?? []);
  dividerId = $derived(this.newFrom[this.selectedJid] ?? null);
  membersHere = $derived(
    this.selectedSpace === HOME ? [] : (this.members[this.selectedSpace] ?? [])
  );
  spaceChannels = $derived(
    this.selectedSpace === HOME
      ? this.channels.filter((c) => c.kind === 'dm')
      : this.channels.filter((c) => c.space === this.selectedSpace)
  );
  currentSpace = $derived(this.spaces.find((s) => spaceKey(s) === this.selectedSpace) ?? null);
  typingHere = $derived(this.typing[this.selectedJid] ?? []);

  spaceOf(key: string): SpaceItem | undefined {
    return this.spaces.find((s) => spaceKey(s) === key);
  }

  /** Totals for the rail. Muted channels do not count. */
  spaceBadge(key: string): { unread: number; mentions: number } {
    let unread = 0;
    let mentions = 0;
    for (const c of this.channels) {
      const inSpace = key === HOME ? c.kind === 'dm' : c.space === key;
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
    const list = key === HOME ? this.channels.filter((c) => c.kind === 'dm') : this.channels.filter((c) => c.space === key);
    const remembered = this.lastChannel[key];
    const target = list.find((c) => c.jid === remembered) ?? list[0];
    this.selectedJid = target?.jid ?? '';
    this.enterChannel();
  }

  selectChannel(jid: string) {
    const c = this.channels.find((x) => x.jid === jid);
    if (!c) return;
    if (jid === this.selectedJid) return;
    this.leaveChannel();
    this.selectedSpace = c.kind === 'dm' ? HOME : (c.space ?? HOME);
    this.selectedJid = jid;
    this.lastChannel[this.selectedSpace] = jid;
    this.enterChannel();
  }

  private leaveChannel() {
    this.replyingTo = null;
    this.editingId = null;
    // Leaving a channel reads it.
    this.newFrom[this.selectedJid] = null;
  }

  private enterChannel() {
    this.lastChannel[this.selectedSpace] = this.selectedJid;
    const c = this.channels.find((x) => x.jid === this.selectedJid);
    if (c) {
      c.unread = 0;
      c.mentions = 0;
    }
  }

  /** Step through the channels of the current circle. */
  step(dir: 1 | -1) {
    const list = this.spaceChannels;
    if (!list.length) return;
    const i = list.findIndex((c) => c.jid === this.selectedJid);
    const next = list[(i + dir + list.length) % list.length];
    this.selectChannel(next.jid);
  }

  /** Step to the next channel with unread messages. Looks at all circles. */
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

  markRead(jid: string = this.selectedJid) {
    const c = this.channels.find((x) => x.jid === jid);
    if (c) {
      c.unread = 0;
      c.mentions = 0;
    }
    this.newFrom[jid] = null;
  }

  // --- messages ----------------------------------------------------

  private list(jid: string): TimelineItem[] {
    if (!this.timelines[jid]) this.timelines[jid] = [];
    return this.timelines[jid];
  }

  send(body: string) {
    const text = body.trim();
    if (!text) return;
    const list = this.list(this.selectedJid);
    const prev = list[list.length - 1];
    const reply = this.replyingTo;
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
    this.replyingTo = null;
    // Sample data: the server confirms a moment later.
    setTimeout(() => {
      const m = this.timelines[this.selectedJid]?.find((x) => x.id === id);
      if (m) m.status = 'sent';
    }, 500);
  }

  /** Attach a file. Sample data: the file stays local. */
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

  edit(id: string, body: string) {
    const m = this.items.find((x) => x.id === id);
    const text = body.trim();
    if (!m || !text) return;
    if (text !== m.body) {
      m.body = text;
      m.edited = true;
    }
    this.editingId = null;
  }

  retract(id: string) {
    const m = this.items.find((x) => x.id === id);
    if (m) {
      m.retracted = true;
      m.attachment = null;
      m.reactions = [];
    }
  }

  toggleReaction(id: string, emoji: string) {
    const m = this.items.find((x) => x.id === id);
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

  lastOwn(): TimelineItem | null {
    for (let i = this.items.length - 1; i >= 0; i--) {
      const m = this.items[i];
      if (m.outgoing && !m.retracted) return m;
    }
    return null;
  }

  startReply(m: TimelineItem) {
    this.editingId = null;
    this.replyingTo = m;
  }

  // --- circles -----------------------------------------------------

  setShow(show: Show) {
    this.me.show = show;
  }

  createCircle(name: string): string {
    const clean = name.trim();
    const node = clean.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'circle';
    const s: SpaceItem = { service: 'chat.foid.space', node, name: clean, avatar: null };
    return this.addCircle(s, 'Say hello.');
  }

  joinCircle(p: PublicCircle): string {
    return this.addCircle(
      { service: p.service, node: p.node, name: p.name, avatar: null },
      p.description
    );
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

  createChannel(space: string, name: string) {
    const clean = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
    if (!clean) return;
    const c = this.newChannel(space, clean, null);
    if (!this.channels.some((x) => x.jid === c.jid)) this.channels.push(c);
    this.selectChannel(c.jid);
  }

  leaveCircle(key: string) {
    this.leaveChannel();
    this.spaces = this.spaces.filter((s) => spaceKey(s) !== key);
    this.channels = this.channels.filter((c) => c.space !== key);
    this.selectedSpace = '';
    this.selectSpace(HOME);
  }

  /** Open a DM with a member. Creates the row when it is missing. */
  openDm(id: string, name: string) {
    if (!this.channels.some((c) => c.jid === id)) {
      const m = Object.values(this.members).flat().find((x) => x.id === id);
      this.channels.push({
        jid: id,
        name,
        kind: 'dm',
        unread: 0,
        joined: true,
        mentions: 0,
        muted: false,
        topic: null,
        space: null,
        avatar: m?.avatar ?? null,
        show: m?.show ?? null,
        online: m?.online ?? false
      });
    }
    this.selectChannel(id);
  }
}

export const app = new AppState();
