// One shared clock for all relative timestamps. The timer runs only while at least one
// timestamp is on screen, and it runs once, not once per timestamp.
import { createSubscriber } from 'svelte/reactivity';

const subscribe = createSubscriber((update) => {
  const id = setInterval(update, 30_000);
  return () => clearInterval(id);
});

/** The current time in ms. Call it inside an effect or a derived value to get updates. */
export function tick(): number {
  subscribe();
  return Date.now();
}
