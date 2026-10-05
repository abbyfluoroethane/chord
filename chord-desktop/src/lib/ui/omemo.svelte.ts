// OMEMO state of the UI: the lock of each chat, the dialogs, the notices, the failed sends.
// Inside the app it calls the bridge. In the browser preview it keeps sample data.
import type { ClientEvent, OmemoDevice, OmemoTrust } from '$lib/chord/types';
import * as fx from '$lib/fixtures/omemo';
import { api, live } from './bridge';
import { plainError } from './adapt';
import { sortDevices } from './omemodata';
import type { TimelineItem } from './types';
import { ui } from './ui.svelte';

export interface TrustNotice {
  jid: string;
  deviceId: number;
  fingerprint: string;
}

class OmemoState {
  /** The lock of each chat. A chat that is not here is off. */
  enabled = $state<Record<string, boolean>>(
    live ? {} : Object.fromEntries(fx.encryptedChats.map((j) => [j, true]))
  );
  /** The contact whose devices dialog is open. */
  devicesFor = $state<string | null>(null);
  /** Notices about new keys that need a decision. */
  notices = $state<TrustNotice[]>([]);
  /** True after the core made new keys for this device. */
  keysReset = $state(false);
  /** The reason of each failed send, by message id. */
  failures = $state<Record<string, string>>(live ? {} : { ...fx.failures });
  /** The devices that a send left out, by peer. */
  skipped = $state<Record<string, number[]>>(
    live ? {} : { ...fx.skipped }
  );
  /** A counter for each jid, and for `own`. It grows when the devices or the trust change. */
  changes = $state<Record<string, number>>({});

  private sample = $state<Record<string, OmemoDevice[]>>(structuredClone(fx.devices));
  private sampleOwn = $state<OmemoDevice[]>(structuredClone(fx.ownDevices));

  isOn(jid: string): boolean {
    return this.enabled[jid] === true;
  }

  /** Read the lock of a chat from the core. */
  async load(jid: string): Promise<void> {
    if (!live) return;
    try {
      const on = await (await api()).omemoEnabled(jid);
      this.enabled = { ...this.enabled, [jid]: on };
    } catch {
      /* The lock stays off. The composer works as before. */
    }
  }

  /** Turn the lock on or off. The lock changes only when the core says yes. */
  async setEnabled(jid: string, on: boolean): Promise<void> {
    if (live) {
      try {
        await (await api()).setOmemoEnabled(jid, on);
      } catch (e) {
        ui.say(plainError(e), true);
        return;
      }
    }
    this.enabled = { ...this.enabled, [jid]: on };
  }

  openDevices(jid: string) {
    this.devicesFor = jid;
  }

  closeDevices() {
    this.devicesFor = null;
  }

  /** Open the dialog of a notice, and remove the notices of that contact. */
  openNotice(jid: string) {
    this.dismissNotice(jid);
    this.openDevices(jid);
  }

  dismissNotice(jid: string) {
    this.notices = this.notices.filter((n) => n.jid !== jid);
  }

  /** The devices of a contact, from the store of the core. */
  async devices(jid: string): Promise<OmemoDevice[]> {
    if (!live) return sortDevices(this.sample[jid] ?? []);
    return sortDevices(await (await api()).omemoDevices(jid));
  }

  /** Ask the server for new device lists and keys, then read the devices. */
  async refresh(jid: string): Promise<OmemoDevice[]> {
    if (live) await (await api()).refreshOmemoDevices(jid);
    return this.devices(jid);
  }

  async setTrust(jid: string, fingerprint: string, trust: OmemoTrust): Promise<void> {
    if (live) {
      await (await api()).setOmemoTrust(jid, fingerprint, trust);
    } else {
      const patch = (list: OmemoDevice[]) =>
        list.map((d) => (d.fingerprint === fingerprint ? { ...d, trust } : d));
      this.sample = { ...this.sample, [jid]: patch(this.sample[jid] ?? []) };
      this.sampleOwn = patch(this.sampleOwn);
    }
    this.dismissNotice(jid);
    this.bump(jid);
  }

  /** Our own devices. This device is first. */
  async ownDevices(): Promise<OmemoDevice[]> {
    const list = live ? await (await api()).ownOmemoDevices() : this.sampleOwn;
    return sortDevices(list).sort((a, b) => Number(b.thisDevice) - Number(a.thisDevice));
  }

  async removeOwn(deviceId: number): Promise<void> {
    if (live) await (await api()).removeOwnOmemoDevice(deviceId);
    else this.sampleOwn = this.sampleOwn.filter((d) => d.deviceId !== deviceId);
    this.bump('own');
  }

  /** The reason why an encrypted message did not go out, or null. */
  failureOf(item: Pick<TimelineItem, 'originId' | 'stanzaId' | 'id'>): string | null {
    for (const id of [item.originId, item.stanzaId, item.id]) {
      if (id && this.failures[id]) return this.failures[id];
    }
    return null;
  }

  dismissSkipped(jid: string) {
    const { [jid]: _gone, ...rest } = this.skipped;
    this.skipped = rest;
  }

  /** Forget the failure of a message. */
  clearFailure(item: Pick<TimelineItem, 'originId' | 'stanzaId' | 'id'>) {
    const next = { ...this.failures };
    for (const id of [item.originId, item.stanzaId, item.id]) if (id) delete next[id];
    this.failures = next;
  }

  /** Handle the OMEMO events of the core. */
  onEvent(e: ClientEvent) {
    switch (e.type) {
      case 'omemoKeysReset':
        this.keysReset = true;
        break;
      case 'omemoTrustNeeded': {
        const n = e.data;
        if (!this.notices.some((x) => x.jid === n.jid && x.deviceId === n.deviceId)) {
          this.notices = [...this.notices, n];
        }
        this.bump(n.jid);
        break;
      }
      case 'omemoDevicesChanged':
        this.bump(e.data);
        this.bump('own');
        break;
      case 'omemoDevicesSkipped':
        this.skipped = { ...this.skipped, [e.data.peer]: e.data.deviceIds };
        break;
      case 'omemoSendFailed':
        if (e.data.messageId) this.failures = { ...this.failures, [e.data.messageId]: e.data.reason };
        else ui.say(e.data.reason, true);
        break;
    }
  }

  private bump(key: string) {
    this.changes = { ...this.changes, [key]: (this.changes[key] ?? 0) + 1 };
  }
}

export const omemo = new OmemoState();
