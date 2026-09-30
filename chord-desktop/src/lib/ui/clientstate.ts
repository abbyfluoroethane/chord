// Client State Indication (XEP-0352): tell the server when nobody looks at the window.
// An inactive client gets fewer presence updates and chat states.
//
// When is nobody looking? A hidden page (minimised, or on another desktop) is clear: we
// go inactive at once. A window that only lost focus may still be on screen next to
// another app, so we wait `BLUR_DELAY` before we go inactive. Focus or a visible page
// makes us active at once. A message with a body still reaches us when we are inactive,
// so notifications do not wait.

/** How long the window may stay unfocused, but visible, before it counts as inactive. */
export const BLUR_DELAY = 30_000;

/** The parts of `document` and `window` that we use. Tests give a stand-in. */
export interface Env {
  visible(): boolean;
  focused(): boolean;
  listen(event: 'visibilitychange' | 'focus' | 'blur', handler: () => void): () => void;
  setTimeout(handler: () => void, ms: number): unknown;
  clearTimeout(id: unknown): void;
}

export const browserEnv: Env = {
  visible: () => document.visibilityState !== 'hidden',
  focused: () => document.hasFocus(),
  listen(event, handler) {
    const target = event === 'visibilitychange' ? document : window;
    target.addEventListener(event, handler);
    return () => target.removeEventListener(event, handler);
  },
  setTimeout: (handler, ms) => setTimeout(handler, ms),
  clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>)
};

/**
 * Send the client state when it changes. `send(true)` means active. It sends the first
 * state at once, and only sends a change after that. Returns a function that stops it.
 */
export function watchClientState(send: (active: boolean) => void, env: Env = browserEnv) {
  let sent: boolean | null = null;
  let timer: unknown = null;

  const set = (active: boolean) => {
    if (sent === active) return;
    sent = active;
    send(active);
  };
  const cancel = () => {
    if (timer !== null) env.clearTimeout(timer);
    timer = null;
  };
  const update = () => {
    cancel();
    if (!env.visible()) set(false);
    else if (env.focused()) set(true);
    else timer = env.setTimeout(() => set(false), BLUR_DELAY);
  };

  const stops = (['visibilitychange', 'focus', 'blur'] as const).map((e) => env.listen(e, update));
  update();
  return () => {
    cancel();
    stops.forEach((stop) => stop());
  };
}
