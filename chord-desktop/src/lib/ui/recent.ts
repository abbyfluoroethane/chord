// A short list of the chats that the user opened last. The timelines of older chats are
// dropped from memory. Plain functions, so that Vitest can test them.

/** How many timelines the app keeps in memory. The open chat is one of them. */
export const KEEP_TIMELINES = 8;

/**
 * Put `key` first in `recent`. Returns the new list and the keys that fell off the end.
 * `recent` is not changed.
 */
export function touchRecent(
  recent: readonly string[],
  key: string,
  keep: number = KEEP_TIMELINES
): { recent: string[]; dropped: string[] } {
  const next = [key, ...recent.filter((k) => k !== key)];
  return { recent: next.slice(0, keep), dropped: next.slice(keep) };
}
