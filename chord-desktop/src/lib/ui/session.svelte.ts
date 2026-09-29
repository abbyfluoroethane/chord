// Sign-in and connection state. Sample behaviour until the bridge is wired in.

export type ConnState = 'signed-out' | 'connecting' | 'connected' | 'reconnecting';

export interface Credentials {
  address: string;
  password: string;
  remember: boolean;
  server: string;
}

const REMEMBER_KEY = 'chord.remember';

class Session {
  state = $state<ConnState>('signed-out');
  error = $state<string | null>(null);
  /** The server name the reconnect banner shows. */
  host = $state('foid.space');

  savedAddress(): string {
    try {
      return localStorage.getItem(REMEMBER_KEY) ?? '';
    } catch {
      return '';
    }
  }

  async signIn(c: Credentials): Promise<void> {
    this.error = null;
    const address = c.address.trim();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(address)) {
      this.error = 'Enter your address like you@example.com.';
      return;
    }
    if (!c.password) {
      this.error = 'Enter your password.';
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

  signOut() {
    this.state = 'signed-out';
    this.error = null;
  }

  /** For previews and tests of the banner. */
  force(state: ConnState) {
    this.state = state;
  }
}

export const session = new Session();
