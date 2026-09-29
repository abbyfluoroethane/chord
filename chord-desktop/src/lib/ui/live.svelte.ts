// The live controller: it connects the stores to the bridge. It only loads inside
// Tauri (session.svelte.ts imports it on demand).
//
// It keeps these subscriptions, each with applyDiff:
//   - the space list and the Home channel list, for the whole session
//   - the channel list of each circle (for the rail badges and the quick switcher)
//   - the timeline of the open channel, or its private timeline
//   - the member list of the open room (or the two people of the open chat)
// The view effects unsubscribe when the selection changes.
import { untrack } from 'svelte';
import {
  applyDiff,
  listenEvents,
  subscribeChannelList,
  subscribeMemberList,
  subscribePrivateTimeline,
  subscribeSpaceList,
  subscribeTimeline
} from '$lib/chord';
import { api } from './bridge';
import type {
  ChannelItem as BChannel,
  ClientEvent,
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
import { session } from './session.svelte';
import { settings } from './local';
import { spaceKey, type ChannelItem } from './types';
import { ui } from './ui.svelte';

class LiveController {
  rawSpaces = $state.raw<BSpace[]>([]);
  rawHome = $state.raw<BChannel[]>([]);
  rawBySpace = $state.raw<Record<string, BChannel[]>>({});

  private root: (() => void) | null = null;
  private eventsAttached = false;
  private subs = new Set<ViewSubscription>();
  private spaceSubs = new Map<string, ViewSubscription | null>();
  private wanted = new Set<string>();
  /** room -> nick -> real JID, from the member list. */
  private nickJids: Record<string, Record<string, string>> = {};
  private levelsAsked = new Set<string>();
  private avatarsAsked = new Set<string>();
  private unlistenDrop: (() => void) | null = null;
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
    const track = async <T extends ViewSubscription>(p: Promise<T>): Promise<T> => {
      const sub = await p;
      if (gen !== this.generation) {
        void sub.unsubscribe();
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
        this.rawHome = this.diff(this.rawHome, d);
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
    this.rawSpaces = [];
    this.rawHome = [];
    this.rawBySpace = {};
    this.nickJids = {};
    this.levelsAsked.clear();
    this.avatarsAsked.clear();
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
    contactsStore.contacts = [];
    contactsStore.incoming = [];
    contactsStore.outgoing = [];
    contactsStore.blocked = [];
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
    // One channel list for each circle.
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
      const raw = [...this.rawHome, ...Object.values(this.rawBySpace).flat()];
      untrack(() => {
        for (const c of raw) {
          if (!this.levelsAsked.has(c.jid)) {
            this.levelsAsked.add(c.jid);
            void api().then(async (b) => {
              try {
                app.levels[c.jid] = await b.notificationLevel(c.jid);
              } catch {
                /* the default stays */
              }
            });
          }
          if (c.kind.type === 'direct' && !this.avatarsAsked.has(c.jid)) {
            this.avatarsAsked.add(c.jid);
            void api().then((b) => b.refreshAvatar(c.jid).catch(() => undefined));
          }
        }
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

  private reconcileSpaces(keys: string[]) {
    this.wanted = new Set(keys);
    for (const [key, sub] of [...this.spaceSubs]) {
      if (!this.wanted.has(key)) {
        this.spaceSubs.delete(key);
        void sub?.unsubscribe();
        const rest = { ...this.rawBySpace };
        delete rest[key];
        this.rawBySpace = rest;
      }
    }
    const gen = this.generation;
    for (const key of keys) {
      if (this.spaceSubs.has(key)) continue;
      this.spaceSubs.set(key, null);
      subscribeChannelList(scopeOf(key), (d) => {
        if (gen !== this.generation) return;
        this.rawBySpace = { ...this.rawBySpace, [key]: this.diff(this.rawBySpace[key] ?? [], d) };
      })
        .then((sub) => {
          if (gen !== this.generation || !this.wanted.has(key)) void sub.unsubscribe();
          else this.spaceSubs.set(key, sub);
        })
        .catch((e) => ui.say(plainError(e)));
    }
  }

  private buildChannels(): ChannelItem[] {
    const out: ChannelItem[] = [];
    const seen = new Set<string>();
    const push = (c: BChannel, space: string | null) => {
      if (seen.has(c.jid)) return;
      seen.add(c.jid);
      out.push(
        toChannel(c, {
          space,
          muted: isMuted(app.levels[c.jid]),
          mentions: app.mentions[c.jid] ?? 0,
          presence: app.dmPresence[c.jid] ?? null
        })
      );
    };
    for (const c of this.rawHome) push(c, null);
    for (const [key, list] of Object.entries(this.rawBySpace)) for (const c of list) push(c, key);
    for (const l of app.localChannels) {
      if (!seen.has(l.jid)) {
        seen.add(l.jid);
        out.push({ ...l, muted: isMuted(app.levels[l.jid]) });
      }
    }
    return out;
  }

  private watchTimeline(jid: string, isRoom: boolean): () => void {
    let cancelled = false;
    let sub: TimelineSubscription | null = null;
    let first = true;
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
      try {
        app.timelines[jid] = applyDiff(app.timelines[jid] ?? [], mapped);
      } catch (e) {
        console.warn('chord: bad timeline diff', e);
        return;
      }
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
        await s.unsubscribe();
        return;
      }
      sub = s;
      app.timelineSub = s;
    })().catch((e) => ui.say(plainError(e)));
    return () => {
      cancelled = true;
      if (app.timelineSub === sub) app.timelineSub = null;
      void sub?.unsubscribe();
    };
  }

  private watchMembers(jid: string, c: ChannelItem): () => void {
    let cancelled = false;
    let sub: ViewSubscription | null = null;
    let raw: BMember[] = [];
    const room = c.kind === 'channel';
    const space = c.space ?? HOME;
    const onDiff = (d: ListDiff<BMember>) => {
      if (cancelled) return;
      raw = this.diff(raw, d);
      if (room) {
        app.members[space] = raw.map((m) => toMember(m, jid));
        this.nickJids[jid] = Object.fromEntries(raw.filter((m) => m.jid).map((m) => [m.id, m.jid!]));
      } else {
        const peer = raw.find((m) => m.jid === jid || m.id === jid);
        if (peer) app.dmPresence[jid] = { show: toShow(peer.show), online: peer.online };
      }
    };
    void subscribeMemberList(jid, onDiff)
      .then(async (s) => {
        if (cancelled) await s.unsubscribe();
        else sub = s;
      })
      .catch((e) => ui.say(plainError(e)));
    return () => {
      cancelled = true;
      void sub?.unsubscribe();
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
        if (!n.mention) break;
        app.mentionIds[n.itemId] = true;
        const looking = n.peer === app.selectedJid && document.hasFocus();
        if (!looking) app.mentions[n.peer] = (app.mentions[n.peer] ?? 0) + 1;
        break;
      }
      case 'subscriptionRequest':
        contactsStore.addIncoming(e.data);
        break;
      case 'roomInvite':
        ui.say(`${localPart(e.data.from)} invited you to ${localPart(e.data.room)}.`);
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
      case 'messageReceived':
        // The views already show it.
        break;
    }
  }

  /** A file dropped on the window goes to the open chat. Rust reads it from its path. */
  private async listenDrops(gen: number) {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      const un = await getCurrentWebview().onDragDropEvent((ev) => {
        if (ev.payload.type !== 'drop' || app.showContacts) return;
        for (const p of ev.payload.paths) void app.uploadPath(p);
      });
      if (gen !== this.generation) un();
      else this.unlistenDrop = un;
    } catch {
      /* no drag and drop: the upload button explains it */
    }
  }
}

export const liveController = new LiveController();
