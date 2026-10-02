// When the "You are viewing older messages" bar shows. The bar needs a reader who is far
// from the present: this many messages sit below the bottom edge of the view.

/** The count of messages below the view at which the bar shows. */
export const OLDER_BAR_MESSAGES = 50;

/**
 * Count the messages that start at or below `viewBottom`. `count` is the number of
 * messages, `topOf(i)` gives the top of message `i`, and the tops grow with `i`.
 */
export function messagesBelow(count: number, topOf: (i: number) => number, viewBottom: number): number {
  let lo = 0;
  let hi = count;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (topOf(mid) >= viewBottom) hi = mid;
    else lo = mid + 1;
  }
  return count - lo;
}

/** True when the reader is far enough from the present to see the bar. */
export function showOlderBar(below: number, limit = OLDER_BAR_MESSAGES): boolean {
  return below >= limit;
}
