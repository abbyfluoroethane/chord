// Sign-in and connection state. In the browser preview this is sample behaviour.
// Inside Tauri it calls open, login and logout of the bridge and follows the
// connection events (see live.svelte.ts).
import type { ConnectionState } from '$lib/chord/types';
import { authFailureText, connectErrorText, plainError } from './adapt';
import { api, live } from './bridge';
import { settings } from './local';
import { ui } from './ui.svelte';

/**
 * `restoring` means the app has a saved account and signs in with the saved password.
 * The splash screen shows in this state, and not the login form.
 */
export type ConnState = 'signed-out' | 'restoring' | 'connecting' | 'connected' | 'reconnecting';

export interface Credentials {
  address: string;
  password: string;
  remember: boolean;
  server: string;
}

const REMEMBER_KEY = 'chord.remember';
const ADDRESS = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

/** The login field takes `host:port`. The bridge wants `starttls://host:port`. */
export function serverArg(input: string): string {
  const s = input.trim();
  if (!s) return '';
  if (s.startsWith('starttls://')) return s;
  return `starttls://${s.includes(':') ? s : `${s}:5222`}`;
}

async function controller() {
  return (await import('./live.svelte')).liveController;
}

class Session {
  state = $state<ConnState>('signed-out');
  error = $state<string | null>(null);
  /** The server name the reconnect banner shows. */
  host = $state('foid.space');
  /** True while the app reads its settings and decides on the login. */
  booting = $state(live);
  /** The account that is open (live only). */
  address = $state('');
  /** The keychain holds a password for the account (live only). */
  hasSavedPassword = $state(false);

  private lastAddress = '';
  private everConnected = false;
  private leaving = false;
  /** True after the user gave up on the automatic sign-in. */
  private cancelled = false;

  /** True while the splash screen must show: the boot, or the automatic sign-in. */
  get splash(): boolean {
    return this.booting || this.state === 'restoring';
  }

  /** Whether the banner can offer "Try now". */
  get canRetry(): boolean {
    return !live || this.hasSavedPassword;
  }

  savedAddress(): string {
    if (live) return this.lastAddress;
    try {
      return localStorage.getItem(REMEMBER_KEY) ?? '';
    } catch {
      return '';
    }
  }

  /** Live only: read the settings, then log in by itself if a password is saved. */
  async boot(): Promise<void> {
    if (!live) return;
    let auto: string | null = null;
    try {
      await settings.load();
      const s = settings.get<{ account?: string; auto?: boolean }>('session');
      this.lastAddress = s?.account ?? '';
      await (await controller()).attachEvents();
      if (s?.account && s.auto !== false && (await (await api()).savedPassword(s.account))) {
        auto = s.account;
      }
    } catch (e) {
      this.error = plainError(e);
    }
    // Set `restoring` before `booting` ends. The login form must never show in between.
    if (auto) {
      this.host = auto.split('@')[1] ?? this.host;
      this.state = 'restoring';
    }
    this.booting = false;
    if (auto) await this.connect(auto, { password: undefined, server: '', remember: false }, true);
  }

  /** The user gives up on the automatic sign-in and goes to the login form. */
  async cancelRestore(): Promise<void> {
    if (this.state !== 'restoring') return;
    this.cancelled = true;
    this.state = 'signed-out';
    this.error = null;
    try {
      await (await api()).logout();
      await (await controller()).stopViews();
    } catch {
      /* nothing was running */
    }
  }

  async signIn(c: Credentials): Promise<void> {
    this.error = null;
    const address = c.address.trim();
    if (!ADDRESS.test(address)) {
      this.error = 'Enter your address like you@example.com.';
      return;
    }
    if (!c.password) {
      this.error = 'Enter your password.';
      return;
    }
    if (live) {
      await this.connect(address, { password: c.password, server: c.server, remember: c.remember });
      return;
    }
    this.state = 'connecting';
    this.host = c.server.trim() || address.split('@')[1];
    await new Promise((r) => setTimeout(r, 900));
    if (this.host.startsWith('offline')) {
      this.state = 'signed-out';
      this.error = `Can't reach ${this.host}. Check the address and your connection.`;
      return;
    }
    if (c.password === 'wrong') {
      this.state = 'signed-out';
      this.error = 'Wrong address or password.';
      return;
    }
    try {
      if (c.remember) localStorage.setItem(REMEMBER_KEY, address);
      else localStorage.removeItem(REMEMBER_KEY);
    } catch {
      /* ignore */
    }
    this.state = 'connected';
  }

  private async connect(
    address: string,
    o: { password: string | undefined; server: string; remember: boolean },
    restore = false
  ): Promise<void> {
    this.state = restore ? 'restoring' : 'connecting';
    this.error = null;
    this.leaving = false;
    this.cancelled = false;
    this.everConnected = false;
    if (!restore) this.host = o.server.trim().replace(/^starttls:\/\//, '').replace(/:\d+$/, '') || address.split('@')[1];
    try {
      const b = await api();
      const c = await controller();
      const info = await b.open(address);
      this.address = info.account;
      this.hasSavedPassword = info.hasSavedPassword;
      c.setAccount(info.account);
      await b.login({ password: o.password, server: serverArg(o.server), remember: o.remember });
      if (o.remember) this.hasSavedPassword = true;
      this.lastAddress = info.account;
      settings.set('session', { account: info.account, auto: this.hasSavedPassword });
      await c.startViews();
      // The connection event can be first. Do not undo a "reconnecting".
      if (this.cancelled) return;
      if (this.state === 'connecting' || this.state === 'restoring') this.state = 'connected';
      this.everConnected = true;
    } catch (e) {
      if (!this.cancelled) await this.fail(plainError(e));
    }
  }

  /** Back to the login screen, with the reason. */
  private async fail(text: string) {
    this.state = 'signed-out';
    this.error = text;
    try {
      await (await controller()).stopViews();
    } catch {
      /* nothing was running */
    }
  }

  /** Follow a `connectionState` event. */
  applyConnection(s: ConnectionState) {
    if (this.leaving || this.state === 'signed-out') return;
    switch (s.type) {
      case 'connecting':
        if (this.state === 'restoring') break;
        this.state = this.everConnected ? 'reconnecting' : 'connecting';
        break;
      case 'connected': {
        this.everConnected = true;
        this.state = 'connected';
        const at = s.data.boundJid.split('@')[1]?.split('/')[0];
        if (at) this.host = at;
        break;
      }
      case 'suspended':
        this.state = 'reconnecting';
        break;
      case 'loginFailed':
        if (s.data.type === 'authFailed' || !this.everConnected) {
          void this.fail(connectErrorText(s.data, this.host));
        } else {
          this.state = 'reconnecting';
        }
        break;
      case 'authFailed':
        void this.fail(authFailureText(s.data));
        break;
      case 'disconnected':
        if (this.state === 'connected') this.state = 'reconnecting';
        break;
    }
  }

  /** The "Try now" button of the banner. */
  async retry(): Promise<void> {
    if (!live) {
      this.state = 'connected';
      return;
    }
    try {
      await (await api()).login({});
    } catch (e) {
      ui.say(plainError(e));
    }
  }

  async signOut(forget = false): Promise<void> {
    if (!live) {
      this.state = 'signed-out';
      this.error = null;
      return;
    }
    this.leaving = true;
    try {
      const b = await api();
      await b.logout();
      if (forget && this.address) {
        await b.forgetPassword(this.address);
        this.hasSavedPassword = false;
      }
    } catch (e) {
      ui.say(plainError(e));
    }
    settings.set('session', { account: this.address, auto: false });
    await settings.flush();
    try {
      await (await controller()).stopViews();
    } catch {
      /* nothing was running */
    }
    this.state = 'signed-out';
    this.error = null;
  }

  /** For previews and tests of the banner. */
  force(state: ConnState) {
    this.state = state;
  }
}

export const session = new Session();
