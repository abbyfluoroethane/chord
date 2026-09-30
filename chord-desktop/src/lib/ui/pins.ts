// Pinned messages: the rules that need no page, so they are easy to test.
import type { Pin } from '$lib/chord/types';
import type { TimelineItem } from './types';

/** The pin that belongs to `item`, if there is one. A pin made on another device has no
 * link to the row, so it also matches by the id that the server gave. */
export function pinOf(pins: Pin[], item: TimelineItem): Pin | undefined {
  return pins.find(
    (p) =>
      p.itemId === item.id ||
      (p.key !== '' && (p.key === item.stanzaId || p.key === item.originId))
  );
}

/** The pin for `item` in `chat`. Sample data uses it, and the app makes the same fields. */
export function pinFromItem(chat: string, item: TimelineItem, now: number): Pin {
  return {
    chat,
    key: item.stanzaId || item.originId || item.id,
    itemId: item.id,
    sender: item.senderName,
    body: item.body.slice(0, 500),
    timestamp: item.timestamp,
    pinnedAt: now
  };
}

/** Put `pin` first, and drop an older pin of the same message. */
export function addPin(pins: Pin[], pin: Pin): Pin[] {
  return [pin, ...pins.filter((p) => !(p.chat === pin.chat && p.key === pin.key))];
}

/** A short line for the list: one line of the body, cut at `max` characters. */
export function pinLine(pin: Pin, max = 140): string {
  const flat = pin.body.replace(/\s+/g, ' ').trim();
  if (!flat) return 'Attachment';
  return flat.length > max ? flat.slice(0, max - 1) + '…' : flat;
}
