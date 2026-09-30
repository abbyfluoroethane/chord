// xmpp: links. One place decides what a link does: a click in a message, and a link that
// the OS gives to the app. Chord never acts on a link alone. It asks the user first, in
// the dialog of XmppLinkModal. A link that arrives before the sign-in waits in a queue.
//
// The cards under a message ask the server what a space or a room is (`info`). The answer
// stays for the session. In the browser preview the answer comes from the sample data.
import * as fx from '$lib/fixtures/data';
import { plainError } from './adapt';
import { api, live } from './bridge';
import { app } from './app.svelte';
import { contactsStore } from './contacts.svelte';
import { session } from './session.svelte';
import { spaceKey } from './types';
import { ui } from './ui.svelte';
import { parseXmppUri, xmppKey, type KnownXmppLink } from './xmppuri';

/** What the server told us about the target of a link. */
export type LinkInfo =
  | { kind: 'space'; name: string; description: string | null; channels: number | null }
  | {
      kind: 'room';
      name: string;
      subject: string | null;
      occupants: number | null;
      passwordProtected: boolean;
    };

/** `undefined` while the answer is on its way, `null` when the link is not valid. */
type Entry = LinkInfo | null | undefined;

/** A second copy of the same link within this time is ignored. macOS can send a link twice. */
const REPEAT_MS = 1500;

class XmppLinks {
  /** The link that waits for the answer of the user. */
  asking = $state<KnownXmppLink | null>(null);
  private entries = $state<Record<string, Entry>>({});
  private started = new Set<string>();
  private queued: string[] = [];
  private last = { uri: '', at: 0 };

  // --- opening ---------------------------------------------------------

  /**
   * A link from a click or from the OS. Wait for the sign-in, then ask the user.
   * `fromOs` links that repeat within a short time count once.
   */
  open(uri: string, fromOs = false) {
    const now = Date.now();
    if (fromOs && uri === this.last.uri && now - this.last.at < REPEAT_MS) return;
    if (fromOs) this.last = { uri, at: now };
    if (parseXmppUri(uri).kind === 'unknown') {
      ui.say('This link is not valid.');
      return;
    }
    if (session.state !== 'connected') {
      this.queued.push(uri);
      return;
    }
    this.ask(uri);
  }

  /** Ask about the queued links, one after the other. Call it when the session is up. */
  flush() {
    if (session.state !== 'connected' || this.asking) return;
    const next = this.queued.shift();
    if (next) this.ask(next);
  }

  private ask(uri: string) {
    const link = parseXmppUri(uri);
    if (link.kind === 'unknown') return;
    this.asking = link;
    this.request(link);
  }

  /** The user closed the dialog. Show the next link that waits. */
  dismiss() {
    this.asking = null;
    this.flush();
  }

  // --- the answers of the server -----------------------------------------

  /** The answer for a space or a room so far. Call `request` to get it. */
  get(link: KnownXmppLink): Entry {
    return this.entries[xmppKey(link)];
  }

  /** Ask once for a space or a room. A failed request means "not valid" until the app restarts. */
  request(link: KnownXmppLink) {
    if (link.kind !== 'space' && link.kind !== 'room') return;
    const key = xmppKey(link);
    if (this.started.has(key)) return;
    this.started.add(key);
    void this.lookup(link).then((info) => (this.entries[key] = info));
  }

  private async lookup(link: KnownXmppLink): Promise<LinkInfo | null> {
    if (!live) {
      await new Promise((r) => setTimeout(r, 200));
      return this.sample(link);
    }
    try {
      const b = await api();
      if (link.kind === 'space') {
        const key = spaceKey(link);
        // A space that we follow has its name and rooms here. Its service may not answer a stranger.
        const mine = app.spaces.find((s) => spaceKey(s) === key);
        if (mine) {
          const channels = app.channels.filter((c) => c.space === key && c.kind === 'channel').length;
          return { kind: 'space', name: mine.name, description: null, channels };
        }
        const c = await b.spaceInfo(link.service, link.node);
        return { kind: 'space', name: c.name, description: c.description, channels: c.channels };
      }
      if (link.kind === 'room') {
        const c = await b.roomInfo(link.jid);
        return {
          kind: 'room',
          name: c.name ?? link.jid.split('@')[0],
          subject: c.subject ?? c.description,
          occupants: c.occupants,
          passwordProtected: c.passwordProtected
        };
      }
    } catch {
      /* The target does not exist, or the server refuses. */
    }
    // A room that we know already: its own row is enough.
    if (link.kind === 'room') {
      const known = app.channels.find((c) => c.jid === link.jid && c.kind === 'channel');
      if (known) {
        return { kind: 'room', name: known.name, subject: known.topic, occupants: null, passwordProtected: false };
      }
    }
    return null;
  }

  /** The sample data answers in the browser preview. */
  private sample(link: KnownXmppLink): LinkInfo | null {
    if (link.kind === 'space') {
      const key = spaceKey(link);
      const pub = fx.publicCircles.find((p) => spaceKey(p) === key);
      const own = fx.spaces.find((s) => spaceKey(s) === key);
      const name = pub?.name ?? own?.name;
      if (!name) return null;
      const channels = fx.channels.filter((c) => c.space === key && c.kind === 'channel').length;
      return { kind: 'space', name, description: pub?.description ?? null, channels: own ? channels : 3 };
    }
    if (link.kind === 'room') {
      const known = fx.channels.find((c) => c.jid === link.jid && c.kind === 'channel');
      const extra = fx.xmppRooms[link.jid];
      if (known) {
        const here = known.space ? (fx.members[known.space] ?? []).filter((m) => m.online).length : 0;
        return { kind: 'room', name: known.name, subject: known.topic, occupants: here, passwordProtected: false };
      }
      if (extra) return { kind: 'room', ...extra, passwordProtected: false };
    }
    return null;
  }

  // --- state of the app ----------------------------------------------------

  /** True when we are in the space or the room of the link already. */
  joined(link: KnownXmppLink): boolean {
    if (link.kind === 'space') return !!app.spaceOf(spaceKey(link));
    if (link.kind === 'room') {
      return app.channels.some((c) => c.jid === link.jid && c.kind === 'channel' && c.joined);
    }
    return false;
  }

  // --- acting ------------------------------------------------------------------

  /** Do what the link says. Call it only after a click of the user. Returns true on success. */
  async act(link: KnownXmppLink, info: LinkInfo | null | undefined): Promise<boolean> {
    try {
      switch (link.kind) {
        case 'space': {
          const key = spaceKey(link);
          if (app.spaceOf(key)) {
            app.selectSpace(key);
            return true;
          }
          const name = info?.kind === 'space' ? info.name : link.node;
          const description = info?.kind === 'space' ? (info.description ?? '') : '';
          return await app.joinCircleAsync({
            service: link.service,
            node: link.node,
            name,
            description,
            members: null
          });
        }
        case 'room':
          return await app.joinRoomLink(link.jid, link.password);
        case 'chat':
          contactsStore.message(link.jid);
          return true;
        case 'contact': {
          if (contactsStore.isContact(link.jid)) {
            contactsStore.message(link.jid);
            return true;
          }
          const r = contactsStore.add(link.jid);
          ui.say(r.ok ? r.message : r.error);
          return r.ok;
        }
      }
    } catch (e) {
      ui.say(plainError(e));
      return false;
    }
  }
}

export const xmppLinks = new XmppLinks();
