// The reply chain of a message: what it answers, what that answers, and so on up to the
// first message, and the messages that answer it. Plain functions, so that Vitest can
// test them. The timeline gives the order.
import type { TimelineItem } from './types';

/** A message answers `id` when `id` is its id, or an id that the server or the sender gave. */
function hasId(item: TimelineItem, id: string): boolean {
  return id !== '' && (item.id === id || item.stanzaId === id || item.originId === id);
}

/** The message that `item` answers, if the page holds it. */
export function parentOf(items: TimelineItem[], item: TimelineItem): TimelineItem | undefined {
  const id = item.replyTo?.id;
  return id ? items.find((x) => x !== item && hasId(x, id)) : undefined;
}

/**
 * The chain around `item`, in timeline order: the messages it answers (up to the first
 * one that the page holds), itself, and every message that answers one of them.
 * A loop in the data ends the walk.
 */
export function replyChain(items: TimelineItem[], item: TimelineItem): TimelineItem[] {
  const inChain = new Set<TimelineItem>([item]);
  for (let at = parentOf(items, item); at && !inChain.has(at); at = parentOf(items, at)) {
    inChain.add(at);
  }
  // The replies, at any depth. One pass in timeline order is enough when replies come
  // after their target. A second pass catches the rest.
  for (let pass = 0; pass < 2; pass++) {
    for (const x of items) {
      if (inChain.has(x)) continue;
      const p = parentOf(items, x);
      if (p && inChain.has(p)) inChain.add(x);
    }
  }
  return items.filter((x) => inChain.has(x));
}
