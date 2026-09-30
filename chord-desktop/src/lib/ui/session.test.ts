import { beforeEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => ({
  saved: true,
  settingsValue: { account: 'rin@foid.space', auto: true } as unknown,
  loginError: null as unknown,
  loginGate: null as Promise<void> | null
}));

vi.mock('./bridge', () => ({
  live: true,
  api: async () => ({
    savedPassword: async () => h.saved,
    open: async (a: string) => ({ account: a, hasSavedPassword: true }),
    login: async () => {
      if (h.loginGate) await h.loginGate;
      if (h.loginError) throw h.loginError;
    },
    logout: async () => {}
  })
}));
vi.mock('./local', () => ({
  settings: { load: async () => {}, get: () => h.settingsValue, set: () => {}, flush: async () => {} }
}));
vi.mock('./live.svelte', () => ({
  liveController: {
    attachEvents: async () => {},
    setAccount: () => {},
    startViews: async () => {},
    stopViews: async () => {}
  }
}));

const { session } = await import('./session.svelte');

beforeEach(() => {
  h.saved = true;
  h.settingsValue = { account: 'rin@foid.space', auto: true };
  h.loginError = null;
  h.loginGate = null;
  session.state = 'signed-out';
  session.error = null;
  session.booting = true;
});

describe('session boot', () => {
  it('shows the splash and not the login form with a saved account', async () => {
    let release = () => {};
    h.loginGate = new Promise<void>((r) => (release = r));
    const boot = session.boot();
    await vi.waitFor(() => expect(session.booting).toBe(false));
    expect(session.state).toBe('restoring');
    expect(session.splash).toBe(true);
    expect(session.host).toBe('foid.space');
    release();
    await boot;
    expect(session.splash).toBe(false);
  });

  it('signs in with the saved password', async () => {
    await session.boot();
    expect(session.state).toBe('connected');
    expect(session.error).toBeNull();
  });

  it('goes to signed-out with the error when the sign-in fails', async () => {
    h.loginError = new Error('The server is not reachable');
    await session.boot();
    expect(session.state).toBe('signed-out');
    expect(session.error).toBeTruthy();
    expect(session.splash).toBe(false);
  });

  it('goes to signed-out with no error when no account is saved', async () => {
    h.settingsValue = undefined;
    await session.boot();
    expect(session.state).toBe('signed-out');
    expect(session.error).toBeNull();
    expect(session.splash).toBe(false);
  });

  it('goes to signed-out when the keychain has no password', async () => {
    h.saved = false;
    await session.boot();
    expect(session.state).toBe('signed-out');
    expect(session.splash).toBe(false);
  });

  it('ignores a connecting event while it restores', async () => {
    session.state = 'restoring';
    session.applyConnection({ type: 'connecting' });
    expect(session.state).toBe('restoring');
  });

  it('leaves the splash when the user cancels', async () => {
    let release = () => {};
    h.loginGate = new Promise<void>((r) => (release = r));
    const boot = session.boot();
    await vi.waitFor(() => expect(session.state).toBe('restoring'));
    await session.cancelRestore();
    expect(session.state).toBe('signed-out');
    release();
    await boot;
    expect(session.state).toBe('signed-out');
    expect(session.error).toBeNull();
  });
});
