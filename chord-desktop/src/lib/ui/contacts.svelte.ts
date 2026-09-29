// Contacts, requests, and blocked addresses over sample data.
// Each action names the bridge call it maps to. The calls stay as TODO stubs
// until the bridge is wired in (see $lib/chord/api.ts).
import * as fx from '$lib/fixtures/data';
import { app, HOME } from './app.svelte';
import type { Affiliation, ContactItem, ContactsTab, Person } from './types';
import { spaceKey } from './types';

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

const ADDRESS = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

export type AddResult = { ok: true; message: string } | { ok: false; error: string };

class ContactsStore {
  contacts = $state<ContactItem[]>(clone(fx.contacts));
  incoming = $state<ContactItem[]>(clone(fx.incomingRequests));
  outgoing = $state<ContactItem[]>(clone(fx.outgoingRequests));
  blocked = $state<ContactItem[]>(clone(fx.blockedContacts));
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
    return {
      address,
      name: item?.name ?? member?.name ?? dm?.name ?? fallbackName ?? address.split('@')[0],
      avatar: item?.avatar ?? member?.avatar ?? dm?.avatar ?? null,
      show: item?.show ?? member?.show ?? dm?.show ?? null,
      online: item?.online ?? member?.online ?? dm?.online ?? false,
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

  /** Circles you and this address are both in. */
  sharedCircles(address: string) {
    return app.spaces.filter((s) => app.members[spaceKey(s)]?.some((m) => m.id === address));
  }

  sharedContacts(address: string): ContactItem[] {
    const list = fx.sharedContacts[address] ?? [];
    return this.contacts.filter((c) => list.includes(c.address));
  }

  /** Circles where you are owner or admin. You can invite people to these. */
  circlesYouAdmin() {
    return app.spaces.filter((s) => {
      const mine = app.members[spaceKey(s)]?.find((m) => m.id === app.me.address);
      return mine?.affiliation === 'owner' || mine?.affiliation === 'admin';
    });
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
    // TODO: await api.addContact(address)
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
    // TODO: await api.approveSubscription(address)
    const item = this.incoming.find((c) => c.address === address);
    if (!item) return;
    this.incoming = this.incoming.filter((c) => c.address !== address);
    this.contacts.push({ ...item, since: Date.now() });
  }

  /** Ignore an incoming request. Maps to api.denySubscription(jid). */
  ignore(address: string) {
    // TODO: await api.denySubscription(address)
    this.incoming = this.incoming.filter((c) => c.address !== address);
  }

  /** Cancel a request you sent. Maps to api.removeContact(jid). */
  cancel(address: string) {
    // TODO: await api.removeContact(address)
    this.outgoing = this.outgoing.filter((c) => c.address !== address);
  }

  /** Maps to api.removeContact(jid). */
  remove(address: string) {
    // TODO: await api.removeContact(address)
    this.contacts = this.contacts.filter((c) => c.address !== address);
  }

  /** Accept a request from this address before it arrives. Maps to api.preapproveSubscription(jid). */
  preapprove(input: string): AddResult {
    const address = input.trim().toLowerCase();
    if (!ADDRESS.test(address)) {
      return { ok: false, error: 'That does not look like an address. Try sam@chord.example.' };
    }
    if (this.preapproved.includes(address)) return { ok: false, error: 'That address is already approved.' };
    // TODO: await api.preapproveSubscription(address)
    this.preapproved.push(address);
    return { ok: true, message: `${address} is approved. Their request will be accepted.` };
  }

  /** Maps to the coming api.blockContact(jid). Also removes the contact. */
  block(address: string) {
    // TODO: await api.blockContact(address)
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

  /** Maps to the coming api.unblockContact(jid). */
  unblock(address: string) {
    // TODO: await api.unblockContact(address)
    this.blocked = this.blocked.filter((c) => c.address !== address);
  }

  /** Reload the blocked list. Maps to the coming api.blockedContacts(). */
  refreshBlocked() {
    // TODO: this.blocked = (await api.blockedContacts()).map(toItem)
  }

  /** Open the DM with an address. */
  message(address: string, name: string | null = null) {
    const p = this.person(address, name);
    app.openDm(address, p.name, { avatar: p.avatar, show: p.show, online: p.online });
  }
}

export const contactsStore = new ContactsStore();
