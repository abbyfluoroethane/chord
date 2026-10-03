// Auto-away: set the status to Away when nobody used the computer for a while, and set it
// back when the user returns. The idle time comes from the OS where Rust can ask (macOS).
// Elsewhere the page counts input in its own window, so "idle" means "no input in Chord".
//
// Only an online status goes to Away. Away, Do not disturb and Invisible stay as the user
// set them. A status that the user changes during auto-away is not touched again.
import { browserEnv, type Env } from './idle';
import type { Show } from './types';

/** How often the page asks for the idle time. A click in the window ends auto-away at once. */
export const POLL_MS = 15_000;

/** The status of the user, and a way to change it. */
export interface AwayIO {
  show(): Show;
  setShow(show: Show): void;
}

/** The decision logic. `tick` gets the idle time in ms each time it changes. */
export class AutoAway {
  /** The online status to bring back, or undefined when the app did not set Away. */
  private restore: Show | undefined;
  /** The user changed the status during auto-away. Wait for the next activity. */
  private yielded = false;

  constructor(
    private io: AwayIO,
    private afterMs: number
  ) {}

  tick(idleMs: number) {
    if (idleMs < this.afterMs) {
      this.back();
      return;
    }
    if (this.restore !== undefined) {
      // Still away from us? If the user picked another status, leave it.
      if (this.io.show() !== 'away') {
        this.restore = undefined;
        this.yielded = true;
      }
      return;
    }
    const show = this.io.show();
    if (this.yielded || (show !== null && show !== 'chat')) return;
    this.restore = show;
    this.io.setShow('away');
  }

  /** The user is active again. Bring the old status back if Away is still ours. */
  back() {
    this.yielded = false;
    if (this.restore === undefined) return;
    if (this.io.show() === 'away') this.io.setShow(this.restore);
    this.restore = undefined;
  }
}

export interface AwayEnv extends Env {
  /** Seconds since the last input in the whole session, or null when the OS gives none. */
  systemIdle(): Promise<number | null>;
}

/**
 * Watch the idle time and apply auto-away after `minutes`. Returns a function that stops
 * the watch and brings the old status back.
 */
export function watchAutoAway(
  io: AwayIO,
  minutes: number,
  env: AwayEnv = { ...browserEnv, systemIdle: async () => null }
): () => void {
  const away = new AutoAway(io, minutes * 60_000);
  let last = env.now();
  let stopped = false;
  let timer: unknown = null;

  const poll = async () => {
    timer = null;
    let sys: number | null = null;
    try {
      sys = await env.systemIdle();
    } catch {
      /* No system time: use the window. */
    }
    if (stopped) return;
    away.tick(sys === null ? env.now() - last : sys * 1000);
    timer = env.setTimeout(() => void poll(), POLL_MS);
  };
  const activity = () => {
    last = env.now();
    away.back();
  };

  const stopListening = env.listen(activity);
  void poll();
  return () => {
    stopped = true;
    stopListening();
    if (timer !== null) env.clearTimeout(timer);
    away.back();
  };
}
