// One shared clock for all relative timestamps. The timer runs only while at least one
// timestamp is on screen, and it runs once, not once per timestamp. It does not tick while
// the window is hidden. The clock updates once when the window shows again.
import { createSubscriber } from 'svelte/reactivity';

export const TICK_MS = 30_000;

type ClockDoc = Pick<Document, 'hidden' | 'addEventListener' | 'removeEventListener'>;

/** Start the clock. It calls `update` each `TICK_MS` while visible. It returns a stop function. */
export function startClock(update: () => void, doc: ClockDoc): () => void {
  let id: ReturnType<typeof setInterval> | undefined;
  const sync = () => {
    if (doc.hidden) {
      if (id !== undefined) clearInterval(id);
      id = undefined;
      return;
    }
    if (id === undefined) {
      update();
      id = setInterval(update, TICK_MS);
    }
  };
  doc.addEventListener('visibilitychange', sync);
  if (!doc.hidden) id = setInterval(update, TICK_MS);
  return () => {
    doc.removeEventListener('visibilitychange', sync);
    if (id !== undefined) clearInterval(id);
  };
}

const subscribe = createSubscriber((update) => startClock(update, document));

/** The current time in ms. Call it inside an effect or a derived value to get updates. */
export function tick(): number {
  subscribe();
  return Date.now();
}
