// Unread totals and the Esc rule. Plain functions, so that Vitest can test them.
import type { ChannelItem } from './types';

/** The messages that wait in all chats. Muted chats do not count, as on the rail. */
export function totalUnread(channels: Pick<ChannelItem, 'unread' | 'muted'>[]): number {
  let n = 0;
  for (const c of channels) if (!c.muted) n += c.unread;
  return n;
}

/** The title of the page, and of the window: `(3) Chord` when something waits. */
export function pageTitle(count: number): string {
  return count > 0 ? `(${count}) Chord` : 'Chord';
}

/** The channels that a "mark all as read" has to touch. */
export function unreadChannels<T extends Pick<ChannelItem, 'unread' | 'mentions'>>(
  channels: T[]
): T[] {
  return channels.filter((c) => c.unread > 0 || c.mentions > 0);
}

/**
 * Esc marks the open chat as read only when the focus is not in a place where the user
 * types. A stray Esc in the composer must not clear the "new" line.
 */
export function escMarksRead(target: EventTarget | null): boolean {
  const el = target as Partial<HTMLElement> | null;
  if (!el || typeof el.tagName !== 'string') return true;
  const tag = el.tagName.toLowerCase();
  if (tag === 'input' || tag === 'textarea' || tag === 'select') return false;
  return !el.isContentEditable;
}
