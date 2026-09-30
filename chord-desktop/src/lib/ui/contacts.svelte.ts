// Contacts, requests, and blocked addresses. Sample data in the browser preview.
// Inside Tauri the lists come from the bridge (contacts, blockedContacts and the
// subscriptionRequest event). Each action names the bridge call it maps to.
import * as fx from '$lib/fixtures/data';
import { plainError, splitRoster, toContactItem } from './adapt';
import { app, HOME } from './app.svelte';
import { api, live } from './bridge';
import { settings } from './local';
import { prefs } from './prefs.svelte';
import { ui } from './ui.svelte';
import type { Affiliation, ContactItem, ContactsTab, Person } from './types';
import { spaceKey } from './types';

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

const ADDRESS = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

export type AddResult = { ok: true; message: string } | { ok: false; error: string };

class ContactsStore {
  contacts = $state<ContactItem[]>(live ? [] : clone(fx.contacts));
  incoming = $state<ContactItem[]>(live ? [] : clone(fx.incomingRequests));
  outgoing = $state<ContactItem[]>(live ? [] : clone(fx.outgoingRequests));
  blocked = $state<ContactItem[]>(live ? [] : clone(fx.blockedContacts));
  /** Addresses whose requests are accepted before they arrive. */
  preapproved = $state<string[]>([]);

  tab = $state<ContactsTab>('online');
  query = $state('');

  // --- derived -----------------------------------------------------

  online = $derived(this.contacts.filter((c) => c.online));
  pendingCount = $derived(this.incoming.length);

  // --- lookups -----------------------------------------------------

  isContact(address: string): boolean {
    return this.contacts.some((c) => c.address === address);
  }

  isBlocked(address: string): boolean {
    return this.blocked.some((c) => c.address === address);
  }

  /** Gather what is known about an address from every list. */
  person(address: string, fallbackName: string | null = null): Person {
    const me = app.me;
    if (address === me.address) {
      return {
        address,
        name: me.name,
        avatar: me.avatar,
        show: me.show,
        online: true,
        status: null,
        since: null,
        isMe: true,
        isContact: false,
        isBlocked: false,
        affiliation: this.affiliationOf(address),
        role: this.roleOf(address)
      };
    }
    const item =
      this.contacts.find((c) => c.address === address) ??
      this.incoming.find((c) => c.address === address) ??
      this.outgoing.find((c) => c.address === address) ??
      this.blocked.find((c) => c.address === address);
    const member = Object.values(app.members)
      .flat()
      .find((m) => m.id === address);
    const dm = app.channels.find((c) => c.kind === 'dm' && c.jid === address);
    const seen = app.dmPresence[address];
    return {
      address,
      name: item?.name ?? member?.name ?? dm?.name ?? fallbackName ?? address.split('@')[0],
      avatar: item?.avatar ?? member?.avatar ?? dm?.avatar ?? null,
      show: member?.show ?? seen?.show ?? item?.show ?? dm?.show ?? null,
      online: member?.online ?? seen?.online ?? item?.online ?? dm?.online ?? false,
      status: item?.status ?? null,
      since: item?.since ?? null,
      isMe: false,
      isContact: this.isContact(address),
      isBlocked: this.isBlocked(address),
      affiliation: this.affiliationOf(address),
      role: this.roleOf(address)
    };
  }

  private memberHere(address: string) {
    return app.selectedSpace === HOME
      ? undefined
      : app.members[app.selectedSpace]?.find((m) => m.id === address);
  }

  private affiliationOf(address: string): Affiliation | null {
    return this.memberHere(address)?.affiliation ?? null;
  }

  private roleOf(address: string): string | null {
    return this.memberHere(address)?.role ?? null;
  }

  /** Spaces you and this address are both in. */
  sharedCircles(address: string) {
    return app.spaces.filter((s) => app.members[spaceKey(s)]?.some((m) => m.id === address));
  }

  sharedContacts(address: string): ContactItem[] {
    // The bridge does not tell which contacts two people share.
    const list = live ? [] : (fx.sharedContacts[address] ?? []);
    return this.contacts.filter((c) => list.includes(c.address));
  }

  /** Spaces where you are owner or admin. You can invite people to these. */
  circlesYouAdmin() {
    return app.spaces.filter((s) => {
      const mine = app.members[spaceKey(s)]?.find((m) => m.id === app.me.address);
      return mine?.affiliation === 'owner' || mine?.affiliation === 'admin';
    });
  }

  // --- live data ---------------------------------------------------

  /** Read the roster and the blocklist. Maps to api.contacts() and api.blockedContacts(). */
  async refresh() {
    if (!live) return;
    try {
      const b = await api();
      const { contacts, outgoing } = splitRoster(await b.contacts());
      this.contacts = contacts;
      this.outgoing = outgoing;
      // A request that was answered is no longer pending.
      this.incoming = this.incoming.filter((i) => !contacts.some((c) => c.address === i.address));
    } catch (e) {
      ui.say(plainError(e));
    }
    await this.refreshBlocked();
  }

  /** A `subscriptionRequest` event. */
  addIncoming(jid: string) {
    const address = jid.split('/')[0].toLowerCase();
    if (this.isBlocked(address) || this.isContact(address)) return;
    if (this.incoming.some((c) => c.address === address)) return;
    if (prefs.autoApprove || this.preapproved.includes(address)) {
      this.act(async (b) => b.approveSubscription(address));
      return;
    }
    this.incoming.push(toContactItem(address, null, Date.now()));
  }

  loadLocal() {
    if (live) this.preapproved = settings.get<string[]>('preapproved') ?? [];
  }

  /** Run a bridge call for an action, tell about a failure, and read the lists again. */
  private act(f: (b: Awaited<ReturnType<typeof api>>) => Promise<unknown>, unsupported?: string) {
    if (!live) return;
    void (async () => {
      try {
        await f(await api());
      } catch (e) {
        const code = (e as { code?: string } | null)?.code;
        ui.say(code === 'unsupported' && unsupported ? unsupported : plainError(e));
      }
      await this.refresh();
    })();
  }

  // --- actions -----------------------------------------------------

  /** Send a contact request. Maps to api.addContact(jid). */
  add(input: string): AddResult {
    const address = input.trim().toLowerCase();
    if (!ADDRESS.test(address)) {
      return { ok: false, error: 'That does not look like an address. Try sam@chord.example.' };
    }
    if (address === app.me.address) return { ok: false, error: 'That is your own address.' };
    if (this.isContact(address)) return { ok: false, error: `${address} is already a contact.` };
    if (this.outgoing.some((c) => c.address === address)) {
      return { ok: false, error: `You already sent a request to ${address}.` };
    }
    if (this.isBlocked(address)) return { ok: false, error: 'You blocked this address. Unblock it first.' };
    if (this.incoming.some((c) => c.address === address)) {
      this.accept(address);
      return { ok: true, message: `${address} had asked already. You are now contacts.` };
    }
    this.act(async (b) => b.addContact(address));
    this.outgoing.push({
      address,
      name: address.split('@')[0],
      avatar: null,
      show: null,
      online: false,
      status: null,
      since: null
    });
    return { ok: true, message: `Request sent to ${address}.` };
  }

  /** Accept an incoming request. Maps to api.approveSubscription(jid). */
  accept(address: string) {
    this.act(async (b) => b.approveSubscription(address));
    const item = this.incoming.find((c) => c.address === address);
    if (!item) return;
    this.incoming = this.incoming.filter((c) => c.address !== address);
    this.contacts.push({ ...item, since: Date.now() });
  }

  /** Ignore an incoming request. Maps to api.denySubscription(jid). */
  ignore(address: string) {
    this.act(async (b) => b.denySubscription(address));
    this.incoming = this.incoming.filter((c) => c.address !== address);
  }

  /** Cancel a request you sent. Maps to api.removeContact(jid). */
  cancel(address: string) {
    this.act(async (b) => b.removeContact(address));
    this.outgoing = this.outgoing.filter((c) => c.address !== address);
  }

  /** Give a contact a new name, for you only. Maps to api.renameContact(jid, name). */
  rename(address: string, name: string) {
    const clean = name.trim();
    this.act(async (b) => b.renameContact(address, clean || null));
    const item = this.contacts.find((c) => c.address === address);
    if (item) item.name = clean || address.split('@')[0];
  }

  /** Maps to api.removeContact(jid). */
  remove(address: string) {
    this.act(async (b) => b.removeContact(address));
    this.contacts = this.contacts.filter((c) => c.address !== address);
  }

  /** Accept a request from this address before it arrives. Maps to api.preapproveSubscription(jid). */
  preapprove(input: string): AddResult {
    const address = input.trim().toLowerCase();
    if (!ADDRESS.test(address)) {
      return { ok: false, error: 'That does not look like an address. Try sam@chord.example.' };
    }
    if (this.preapproved.includes(address)) return { ok: false, error: 'That address is already approved.' };
    this.act(async (b) => b.preapproveSubscription(address));
    this.preapproved.push(address);
    if (live) settings.set('preapproved', $state.snapshot(this.preapproved));
    return { ok: true, message: `${address} is approved. Their request will be accepted.` };
  }

  /** Maps to api.blockContact(jid). Also removes the contact. */
  block(address: string) {
    this.act(async (b) => b.blockContact(address), 'Your server cannot block people.');
    const p = this.person(address);
    this.contacts = this.contacts.filter((c) => c.address !== address);
    this.incoming = this.incoming.filter((c) => c.address !== address);
    if (!this.isBlocked(address)) {
      this.blocked.push({
        address,
        name: p.name,
        avatar: p.avatar,
        show: null,
        online: false,
        status: null,
        since: null
      });
    }
  }

  /** Maps to api.unblockContact(jid). */
  unblock(address: string) {
    this.act(async (b) => b.unblockContact(address), 'Your server cannot block people.');
    this.blocked = this.blocked.filter((c) => c.address !== address);
  }

  /** Reload the blocked list. Maps to api.blockedContacts(). */
  async refreshBlocked() {
    if (!live) return;
    try {
      const list = await (await api()).blockedContacts();
      this.blocked = list.map((jid) => {
        const known = this.contacts.find((c) => c.address === jid);
        return known ?? toContactItem(jid, null);
      });
    } catch (e) {
      ui.say(plainError(e));
    }
  }

  /** Open the DM with an address. */
  message(address: string, name: string | null = null) {
    const p = this.person(address, name);
    app.openDm(address, p.name, { avatar: p.avatar, show: p.show, online: p.online });
  }
}

export const contactsStore = new ContactsStore();
