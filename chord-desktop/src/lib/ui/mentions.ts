// @nick autocomplete in the composer. The pieces that need no page, so they are easy to test.
//
// What goes out is plain text: "@nick ". The core marks a room message as a mention when its
// body has our nick as a whole word (notify.rs, mentions_nick), and "@" counts as a word
// boundary, so "@nick" is a mention for the person who has that nick.

export interface MentionMatch {
  /** Where the "@" is in the text. */
  start: number;
  /** The letters after the "@", up to the caret. */
  query: string;
}

/** The "@query" that ends at `caret`. The "@" must start the text or follow a space. */
export function findMention(text: string, caret: number): MentionMatch | null {
  const m = /(?:^|\s)@([^\s@]{0,32})$/.exec(text.slice(0, caret));
  if (!m) return null;
  return { start: caret - m[1].length - 1, query: m[1] };
}

/** Up to `max` nicks for `query`: names that start with it first, then names that hold it. */
export function suggestNicks(nicks: string[], query: string, max = 8): string[] {
  const q = query.toLowerCase();
  const seen = new Set<string>();
  const starts: string[] = [];
  const holds: string[] = [];
  for (const nick of nicks) {
    const n = nick.toLowerCase();
    if (!nick || seen.has(n)) continue;
    seen.add(n);
    if (n.startsWith(q)) starts.push(nick);
    else if (n.includes(q)) holds.push(nick);
  }
  const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' });
  return [...starts.sort(byName), ...holds.sort(byName)].slice(0, max);
}

/** Put "@nick " in place of the "@query" and give the new text and caret. */
export function insertMention(
  text: string,
  caret: number,
  match: MentionMatch,
  nick: string
): { text: string; caret: number } {
  const insert = `@${nick} `;
  return {
    text: text.slice(0, match.start) + insert + text.slice(caret),
    caret: match.start + insert.length
  };
}
