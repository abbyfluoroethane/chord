// The quick reactions in the message menu: the four emoji that this user sent most.

export const DEFAULT_REACTIONS = ['👍', '❤️', '😂', '👀'];

/** How often each emoji went out through the UI. */
export type ReactionUse = Record<string, number>;

/**
 * The `n` emoji for the quick row. The most used come first (a tie keeps the order of the
 * defaults). The defaults fill the rest.
 */
export function topReactions(use: ReactionUse, n = 4, defaults = DEFAULT_REACTIONS): string[] {
  const rank = (e: string) => {
    const i = defaults.indexOf(e);
    return i < 0 ? defaults.length : i;
  };
  const used = Object.entries(use)
    .filter(([, count]) => count > 0)
    .sort((a, b) => b[1] - a[1] || rank(a[0]) - rank(b[0]))
    .map(([emoji]) => emoji);
  return [...new Set([...used, ...defaults])].slice(0, n);
}

/** A new counts object with one more use of `emoji`. */
export function bumpReaction(use: ReactionUse, emoji: string): ReactionUse {
  return { ...use, [emoji]: (use[emoji] ?? 0) + 1 };
}
