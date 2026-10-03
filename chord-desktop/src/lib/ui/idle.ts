// Last user interaction (XEP-0319): tell the contacts when nobody has used Chord for a
// while. The page sees only input in its own window, so "idle" means "no input in Chord".
// A click, a key, a scroll or a mouse move ends it at once.

/** How long without input before we send the idle time. */
export const IDLE_AFTER = 5 * 60_000;

/** The parts of the page that we use. Tests give a stand-in. */
export interface Env {
  now(): number;
  /** Call `handler` on each input event. Returns a function that stops it. */
  listen(handler: () => void): () => void;
  setTimeout(handler: () => void, ms: number): unknown;
  clearTimeout(id: unknown): void;
}

const INPUT_EVENTS = ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart'] as const;

export const browserEnv: Env = {
  now: () => Date.now(),
  listen(handler) {
    INPUT_EVENTS.forEach((e) => window.addEventListener(e, handler, { passive: true }));
    return () => INPUT_EVENTS.forEach((e) => window.removeEventListener(e, handler));
  },
  setTimeout: (handler, ms) => setTimeout(handler, ms),
  clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>)
};

/**
 * Call `send(sinceMs)` when the user goes idle, with the time of the last input (ms since
 * epoch), and `send(null)` when the user is back. `resend` sends the current state again,
 * for a new session (the server forgets it). `stop` ends the watch. `after` gives the wait
 * in ms, or null when Chord must not share the idle time. Call `retune` when its answer
 * changes.
 */
export function watchIdle(
  send: (sinceMs: number | null) => void,
  env: Env = browserEnv,
  after: () => number | null = () => IDLE_AFTER
) {
  let last = env.now();
  let idle = false;
  let timer: unknown = null;

  const arm = (ms: number) => {
    if (timer !== null) env.clearTimeout(timer);
    timer = env.setTimeout(check, ms);
  };
  const disarm = () => {
    if (timer !== null) env.clearTimeout(timer);
    timer = null;
  };
  // One timer for the whole wait: a mouse move only stores the time.
  function check() {
    timer = null;
    const wait = after();
    if (wait === null) return;
    const left = wait - (env.now() - last);
    if (left > 0) return arm(left);
    idle = true;
    send(last);
  }
  const activity = () => {
    last = env.now();
    if (idle) {
      idle = false;
      send(null);
    }
    const wait = after();
    if (timer === null && wait !== null) arm(wait);
  };

  const stopListening = env.listen(activity);
  const first = after();
  if (first !== null) arm(first);
  return {
    resend() {
      if (idle) send(last);
    },
    /** The wait or the switch changed: end an idle state that no longer holds, then wait again. */
    retune() {
      const wait = after();
      if (idle && (wait === null || env.now() - last < wait)) {
        idle = false;
        send(null);
      }
      if (idle) return;
      if (wait === null) return disarm();
      arm(Math.max(0, wait - (env.now() - last)));
    },
    stop() {
      stopListening();
      disarm();
    }
  };
}

/** "Idle 12 min", "Idle 2 h", or "Idle 3 d". `since` is an xs:dateTime. Null if not idle. */
export function idleLabel(since: string | null, now: number = Date.now()): string | null {
  if (!since) return null;
  const at = Date.parse(since);
  if (Number.isNaN(at)) return null;
  const minutes = Math.max(0, Math.round((now - at) / 60_000));
  if (minutes < 1) return 'Idle';
  if (minutes < 60) return `Idle ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `Idle ${hours} h`;
  return `Idle ${Math.round(hours / 24)} d`;
}
