// The live controller: it connects the stores to the bridge. It only loads inside
// Tauri (session.svelte.ts imports it on demand).
//
// It keeps these subscriptions, each with applyDiff:
//   - the space list and the Home channel list, for the whole session
//   - the channel list of each space (for the rail badges and the quick switcher)
//   - the timeline of the open channel, or its private timeline
//   - the member list of the open room (or the two people of the open chat)
// The view effects unsubscribe when the selection changes.
import { untrack } from 'svelte';
import {
  applyDiff,
  listenEvents,
  reuseRows,
  subscribeChannelList,
  subscribeMemberList,
  subscribePrivateTimeline,
  subscribeSpaceList,
  subscribeTimeline
} from '$lib/chord';
import { api } from './bridge';
import { dropAction } from './filetransfer';
import { beep, shouldChime } from './notices';
import { prefs } from './prefs.svelte';
import type {
  ChannelItem as BChannel,
  ClientEvent,
  NotificationSetting,
  ListDiff,
  MemberItem as BMember,
  SpaceItem as BSpace,
  TimelineItem as BTimeline,
  TimelineSubscription,
  ViewSubscription
} from '$lib/chord';
import { avatarUrl } from '$lib/chord/avatars';
import {
  isMuted,
  localPart,
  mapDiff,
  plainError,
  scopeOf,
  splitPrivate,
  toChannel,
  toMember,
  toShow,
  toSpace,
  toTimelineItem
} from './adapt';
import { app, HOME } from './app.svelte';
import { contactsStore } from './contacts.svelte';
import { rail, type RailEntry } from './rail.svelte';
import { roomAlerts } from './roomalerts.svelte';
import { session } from './session.svelte';
import { settings } from './local';
import { spaceKey, type ChannelItem, type TimelineItem } from './types';
import { ui } from './ui.svelte';
import { tray } from './tray.svelte';
import { emojiPacks } from './emojipacks.svelte';
import { touchRecent } from './recent';

/** Stop a subscription. A failure here is of no use to the user: the view is gone. */
function quiet(sub: ViewSubscription | null | undefined): void {
  void sub?.unsubscribe().catch(() => undefined);
}

/** Wait in ms before a burst of member diffs shows in the list. */
const MEMBER_FLUSH_MS = 50;
/** Wait in ms before a burst of channel list diffs shows in the list. */
const CHANNEL_FLUSH_MS = 16;
/** Wait in ms before a burst of notification levels shows in the list. */
const LEVEL_FLUSH_MS = 25;

/** The fields of a channel row, in the order that the rows compare. */
const ROW_KEYS: (keyof ChannelItem)[] = [
  'name',
  'kind',
  'unread',
  'joined',
  'mentions',
  'muted',
  'topic',
  'space',
  'avatar',
  'show',
  'online',
  'unknownPresence',
  'members',
  'category'
];

/** What `buildChannels` knows about a row: the inputs, and the row that they gave. */
interface RowMemo {
  raw: BChannel;
  space: string | null;
  level: unknown;
  mentions: number;
  presence: unknown;
  row: ChannelItem;
}

/** True when two channel rows show the same. */
function sameRow(a: ChannelItem, b: ChannelItem): boolean {
  for (const k of ROW_KEYS) if (a[k] !== b[k]) return false;
  const x = a.pm ?? null;
  const y = b.pm ?? null;
  return x === y || (!!x && !!y && x.room === y.room && x.nick === y.nick);
}

class LiveController {
  rawSpaces = $state.raw<BSpace[]>([]);
  rawHome = $state.raw<BChannel[]>([]);
  rawBySpace = $state.raw<Record<string, BChannel[]>>({});

  private root: (() => void) | null = null;
  private eventsAttached = false;
  private contactsTimer: ReturnType<typeof setTimeout> | undefined;
  /** The channel lists as the bridge diffs left them. The raw lists above copy them in a burst. */
  private nextHome: BChannel[] = [];
  private nextBySpace = new Map<string, BChannel[]>();
  private listTimer: ReturnType<typeof setTimeout> | undefined;
  /** Notification levels that came in and wait for one write. */
  private levelsNext: Record<string, NotificationSetting> = {};
  private levelsTimer: ReturnType<typeof setTimeout> | undefined;
  /** The inputs and the result of the last row of each channel. */
  private rowMemo = new Map<string, RowMemo>();
  private subs = new Set<ViewSubscription>();
  private spaceSubs = new Map<string, ViewSubscription | null>();
  private wanted = new Set<string>();
  /** room -> nick -> real JID, from the member list. */
  private nickJids: Record<string, Record<string, string>> = {};
  private levelsAsked = new Set<string>();
  private avatarsAsked = new Set<string>();
  private unlistenDrop: (() => void) | null = null;
  /** The chats whose timelines stay in memory, newest first. */
  private recent: string[] = [];
  private generation = 0;

  // --- start and stop ----------------------------------------------

  /** Listen to the client events, once. Also puts the rail layout in the local settings. */
  async attachEvents(): Promise<void> {
    rail.use({
      load: () => settings.get<RailEntry[]>('rail') ?? null,
      save: (layout) => settings.set('rail', layout)
    });
    if (this.eventsAttached) return;
    this.eventsAttached = true;
    await listenEvents((e) => this.onEvent(e));
  }

  setAccount(address: string) {
    app.me.address = address;
    app.me.name = localPart(address);
    app.me.avatar = avatarUrl(address);
  }

  /** Start the views. Call it after a good login. */
  async startViews(): Promise<void> {
    await this.stopViews();
    const gen = ++this.generation;
    this.root = $effect.root(() => this.effects());
    void app.loadPresence();
    void app.loadProfile();
    void emojiPacks.load();
    const track = async <T extends ViewSubscription>(p: Promise<T>): Promise<T> => {
      const sub = await p;
      if (gen !== this.generation) {
        quiet(sub);
      } else this.subs.add(sub);
      return sub;
    };
    await track(
      subscribeSpaceList((d) => {
        if (gen !== this.generation) return;
        this.rawSpaces = this.diff(this.rawSpaces, d);
        app.spaces = this.rawSpaces.map(toSpace);
        app.spacesReady = true;
      })
    );
    await track(
      subscribeChannelList({ type: 'home' }, (d) => {
        if (gen !== this.generation) return;
        this.onHomeDiff(d);
      })
    );
    void contactsStore.refresh();
    void this.listenDrops(gen);
  }

  /** Stop all subscriptions and empty the stores. */
  async stopViews(): Promise<void> {
    this.generation++;
    this.root?.();
    this.root = null;
    const subs = [...this.subs];
    this.subs.clear();
    for (const s of this.spaceSubs.values()) if (s) subs.push(s);
    this.spaceSubs.clear();
    this.wanted.clear();
    this.unlistenDrop?.();
    this.unlistenDrop = null;
    await Promise.all(subs.map((s) => s.unsubscribe().catch(() => undefined)));
    clearTimeout(this.listTimer);
    clearTimeout(this.levelsTimer);
    this.listTimer = undefined;
    this.levelsTimer = undefined;
    this.levelsNext = {};
    this.nextHome = [];
    this.nextBySpace.clear();
    this.rowMemo = new Map();
    this.rawSpaces = [];
    this.rawHome = [];
    this.rawBySpace = {};
    this.nickJids = {};
    this.levelsAsked.clear();
    this.avatarsAsked.clear();
    this.recent = [];
    app.resetSession();
    app.timelineSub = null;
    app.spaces = [];
    app.channels = [];
    app.localChannels = [];
    app.timelines = {};
    app.members = {};
    app.typing = {};
    app.newFrom = {};
    app.levels = {};
    app.mentions = {};
    app.mentionIds = {};
    app.dmPresence = {};
    app.spacesReady = false;
    app.selectedSpace = HOME;
    app.selectedJid = '';
    app.showContacts = true;
    contactsStore.reset();
  }

  /** A diff of the Home channel list. A burst of diffs shows in the UI as one change. */
  private onHomeDiff(d: ListDiff<BChannel>) {
    this.nextHome = this.diff(this.nextHome, d);
    this.scheduleLists();
  }

  /** A diff of the channel list of a space. A burst of diffs shows in the UI as one change. */
  private onSpaceDiff(key: string, d: ListDiff<BChannel>) {
    this.nextBySpace.set(key, this.diff(this.nextBySpace.get(key) ?? [], d));
    this.scheduleLists();
  }

  private scheduleLists() {
    this.listTimer ??= setTimeout(() => this.publishLists(), CHANNEL_FLUSH_MS);
  }

  /** Copy the channel lists to the reactive state. One copy for each burst, not for each diff. */
  private publishLists() {
    this.listTimer = undefined;
    this.rawHome = this.nextHome;
    this.rawBySpace = Object.fromEntries(this.nextBySpace);
  }

  private diff<T>(list: readonly T[], d: ListDiff<T>): T[] {
    try {
      return applyDiff(list, d);
    } catch (e) {
      // A bad index means we lost a diff. The next reset repairs the list.
      console.warn('chord: bad diff', e);
      return list.slice();
    }
  }

  // --- effects -----------------------------------------------------

  private effects() {
    // One channel list for each space.
    $effect(() => {
      const keys = app.spaces.map(spaceKey);
      untrack(() => this.reconcileSpaces(keys));
    });

    // The channel list of the UI.
    $effect(() => {
      const list = this.buildChannels();
      app.channels = list;
      untrack(() => app.ensureSelection());
    });

    // The notification level and the avatar of each channel.
    $effect(() => {
      const home = this.rawHome;
      const bySpace = this.rawBySpace;
      untrack(() => {
        const ask = (list: readonly BChannel[]) => {
          for (const c of list) {
            if (!this.levelsAsked.has(c.jid)) {
              this.levelsAsked.add(c.jid);
              void api().then(async (b) => {
                try {
                  this.levelsNext[c.jid] = await b.notificationLevel(c.jid);
                  // The answers come one by one. One write for a burst of them is one rebuild.
                  this.levelsTimer ??= setTimeout(() => this.flushLevels(), LEVEL_FLUSH_MS);
                } catch {
                  /* the default stays */
                }
              });
            }
            if (c.kind.type === 'direct' && !this.avatarsAsked.has(c.jid)) {
              this.avatarsAsked.add(c.jid);
              void api().then((b) => b.refreshAvatar(c.jid).catch((e) => console.warn('chord: avatar refresh failed', c.jid, e)));
            }
          }
        };
        ask(home);
        for (const list of Object.values(bySpace)) ask(list);
      });
    });

    // The timeline of the open channel.
    $effect(() => {
      const jid = app.selectedSpace === HOME && app.showContacts ? '' : app.selectedJid;
      if (!jid) return;
      const isRoom = untrack(() => app.channels.find((c) => c.jid === jid)?.kind === 'channel');
      return this.watchTimeline(jid, isRoom);
    });

    // The members of the open room, or the presence of the open chat.
    $effect(() => {
      const jid = app.selectedSpace === HOME && app.showContacts ? '' : app.selectedJid;
      if (!jid) return;
      const c = untrack(() => app.channels.find((x) => x.jid === jid));
      if (!c || c.pm) return;
      return this.watchMembers(jid, c);
    });
  }

  private flushLevels() {
    this.levelsTimer = undefined;
    const next = this.levelsNext;
    this.levelsNext = {};
    for (const [jid, level] of Object.entries(next)) app.levels[jid] = level;
  }

  private reconcileSpaces(keys: string[]) {
    this.wanted = new Set(keys);
    for (const [key, sub] of [...this.spaceSubs]) {
      if (!this.wanted.has(key)) {
        this.spaceSubs.delete(key);
        quiet(sub);
        this.nextBySpace.delete(key);
        this.scheduleLists();
      }
    }
    const gen = this.generation;
    for (const key of keys) {
      if (this.spaceSubs.has(key)) continue;
      this.spaceSubs.set(key, null);
      subscribeChannelList(scopeOf(key), (d) => {
        if (gen !== this.generation) return;
        this.onSpaceDiff(key, d);
      })
        .then((sub) => {
          if (gen !== this.generation || !this.wanted.has(key)) quiet(sub);
          else this.spaceSubs.set(key, sub);
        })
        .catch((e) => ui.say(plainError(e), true));
    }
  }

  /**
   * The channel rows of the UI. A row that shows the same as before keeps its old object,
   * so a change of one level or one mention does not redraw the other rows. When no row
   * changed, the old list stays.
   */
  private buildChannels(): ChannelItem[] {
    const before = untrack(() => app.channels);
    const old = new Map<string, ChannelItem>();
    for (const c of before) old.set(c.jid, c);
    const out: ChannelItem[] = [];
    const seen = new Set<string>();
    const keep = (row: ChannelItem) => {
      const was = old.get(row.jid);
      out.push(was && sameRow(was, row) ? was : row);
    };
    const memo = new Map<string, RowMemo>();
    const push = (c: BChannel, space: string | null) => {
      if (seen.has(c.jid)) return;
      seen.add(c.jid);
      const level = app.levels[c.jid];
      const mentions = app.mentions[c.jid] ?? 0;
      const presence = app.dmPresence[c.jid] ?? null;
      const m = this.rowMemo.get(c.jid);
      // The same inputs give the same row: skip the work.
      if (m && m.raw === c && m.space === space && m.level === level && m.mentions === mentions && m.presence === presence) {
        memo.set(c.jid, m);
        if (old.get(c.jid) === m.row) out.push(m.row);
        else keep(m.row);
        return;
      }
      const row = toChannel(c, { space, muted: isMuted(level), mentions, presence });
      memo.set(c.jid, { raw: c, space, level, mentions, presence, row });
      keep(row);
    };
    for (const c of this.rawHome) push(c, null);
    for (const [key, list] of Object.entries(this.rawBySpace)) for (const c of list) push(c, key);
    for (const l of app.localChannels) {
      if (!seen.has(l.jid)) {
        seen.add(l.jid);
        keep({ ...l, muted: isMuted(app.levels[l.jid]) });
      }
    }
    this.rowMemo = memo;
    const same = out.length === before.length && out.every((row, i) => row === before[i]);
    return same ? before : out;
  }

  /**
   * Apply a diff to the timeline of a chat. The timeline is a raw list: the rows are never
   * changed in place. A row that did not change keeps its object, so its row does not redraw.
   * Returns false when the diff was bad.
   */
  private applyTimelineDiff(jid: string, d: ListDiff<TimelineItem>): boolean {
    const list = app.timelines[jid] ?? [];
    try {
      let next = applyDiff(list, d);
      if (d.type === 'reset') next = reuseRows(list, next, (m) => m.id);
      app.setTimeline(jid, next);
      return true;
    } catch (e) {
      console.warn('chord: bad timeline diff', e);
      return false;
    }
  }

  private watchTimeline(jid: string, isRoom: boolean): () => void {
    let cancelled = false;
    let sub: TimelineSubscription | null = null;
    let first = true;
    // A long session must not keep the messages of every chat that it ever showed.
    const { recent, dropped } = touchRecent(this.recent, jid);
    this.recent = recent;
    app.dropTimelines(dropped);
    const pm = splitPrivate(jid);
    const onDiff = (d: ListDiff<BTimeline>) => {
      if (cancelled) return;
      const mapped = mapDiff(d, (t) =>
        toTimelineItem(t, {
          room: isRoom || !!pm,
          me: app.me.address,
          resolve: (nick) => this.nickJids[pm?.room ?? jid]?.[nick] ?? null
        })
      );
      if (!this.applyTimelineDiff(jid, mapped)) return;
      if (first && d.type === 'reset') {
        first = false;
        app.markDivider(jid, app.timelines[jid]);
      }
    };
    void (async () => {
      const s = pm
        ? await subscribePrivateTimeline(pm.room, pm.nick, onDiff)
        : await subscribeTimeline(jid, onDiff);
      if (cancelled) {
        quiet(s);
        return;
      }
      sub = s;
      app.timelineSub = s;
    })().catch((e) => ui.say(plainError(e), true));
    return () => {
      cancelled = true;
      if (app.timelineSub === sub) app.timelineSub = null;
      quiet(sub);
    };
  }

  private watchMembers(jid: string, c: ChannelItem): () => void {
    let cancelled = false;
    let sub: ViewSubscription | null = null;
    let raw: BMember[] = [];
    let flushTimer: ReturnType<typeof setTimeout> | undefined;
    const room = c.kind === 'channel';
    const space = c.space ?? HOME;
    const flush = () => {
      flushTimer = undefined;
      if (cancelled) return;
      if (room) {
        app.setMembers(space, raw.map((m) => toMember(m, jid)));
        this.nickJids[jid] = Object.fromEntries(raw.filter((m) => m.jid).map((m) => [m.id, m.jid!]));
      } else {
        const peer = raw.find((m) => m.jid === jid || m.id === jid);
        if (peer) app.dmPresence[jid] = { show: toShow(peer.show), online: peer.online };
      }
    };
    // Presence comes in bursts. One pass for each burst keeps a big room fast.
    const onDiff = (d: ListDiff<BMember>) => {
      if (cancelled) return;
      raw = this.diff(raw, d);
      if (d.type === 'reset') {
        clearTimeout(flushTimer);
        flush();
      } else flushTimer ??= setTimeout(flush, MEMBER_FLUSH_MS);
    };
    void subscribeMemberList(jid, onDiff)
      .then(async (s) => {
        if (cancelled) quiet(s);
        else sub = s;
      })
      .catch((e) => ui.say(plainError(e), true));
    return () => {
      cancelled = true;
      clearTimeout(flushTimer);
      quiet(sub);
    };
  }

  // --- events ------------------------------------------------------

  private onEvent(e: ClientEvent) {
    switch (e.type) {
      case 'connectionState':
        session.applyConnection(e.data);
        break;
      case 'notification': {
        const n = e.data;
        const looking = n.peer === app.selectedJid && document.hasFocus();
        if (shouldChime(prefs, { room: n.room }, looking)) beep(prefs.soundChoice, prefs.soundVolume);
        if (!n.mention) break;
        app.addMentionId(n.itemId);
        if (!looking) app.mentions[n.peer] = (app.mentions[n.peer] ?? 0) + 1;
        break;
      }
      case 'subscriptionRequest':
        contactsStore.addIncoming(e.data);
        break;
      case 'roomInvite':
        // A card with Accept and Decline. The app joins only after a click.
        roomAlerts.invited(e.data.room, e.data.from, e.data.reason, e.data.password);
        break;
      case 'roomDestroyed':
        roomAlerts.destroyed(e.data.room, e.data.reason, e.data.alternate);
        break;
      case 'roomCaptcha':
        // The join waits until the user answers.
        ui.captchaAsk = { room: e.data.room, form: e.data.form };
        break;
      case 'typing':
        app.typing[e.data.peer] = e.data.typers.map((t) =>
          t.includes('@') ? (contactsStore.person(t.split('/')[0]).name) : t
        );
        break;
      case 'notice':
        ui.say(e.data);
        break;
      case 'blockListChanged':
        void contactsStore.refresh();
        break;
      case 'contactChanged':
        // Presence comes in bursts at login: read the list once per burst.
        clearTimeout(this.contactsTimer);
        this.contactsTimer = setTimeout(() => void contactsStore.refresh(), 300);
        break;
      case 'messageReceived':
        // The views already show it.
        break;
    }
  }

  /** A file dropped on the window goes to the open chat. Rust remembers the path from the drop and reads the file. */
  private async listenDrops(gen: number) {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      const un = await getCurrentWebview().onDragDropEvent((ev) => {
        const action = dropAction(ev.payload.type, app.showContacts);
        app.dropping = action === 'show';
        if (action === 'upload' && ev.payload.type === 'drop') {
          for (const p of ev.payload.paths) for (const m of tray.addPath(app.selectedJid, p)) ui.say(m, true);
        }
      });
      if (gen !== this.generation) un();
      else this.unlistenDrop = un;
    } catch {
      /* no drag and drop: the upload button explains it */
    }
  }
}

export const liveController = new LiveController();
