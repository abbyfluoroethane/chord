// The rules for saved drafts. No state here, so they are easy to test.
//
// The settings file is at most 256 KB, and it holds other data too. So the drafts have
// a size limit: a long draft is cut, and the oldest drafts go first when the total is too big.

/** The most characters of one draft. */
export const MAX_DRAFT_CHARS = 4000;
/** The most characters of all drafts together. At 4 bytes a character this is 160 KB. */
export const MAX_TOTAL_CHARS = 40_000;

export type Drafts = Record<string, string>;

/**
 * The drafts after `jid` got `text`. An empty (or blank) text removes the draft. The draft
 * becomes the newest. Returns a new object.
 */
export function putDraft(drafts: Drafts, jid: string, text: string): Drafts {
  const out: Drafts = {};
  for (const [k, v] of Object.entries(drafts)) if (k !== jid) out[k] = v;
  if (text.trim()) out[jid] = text.slice(0, MAX_DRAFT_CHARS);
  let total = Object.values(out).reduce((n, v) => n + v.length, 0);
  for (const k of Object.keys(out)) {
    if (total <= MAX_TOTAL_CHARS || k === jid) break;
    total -= out[k].length;
    delete out[k];
  }
  return out;
}

/** Read the saved value. Anything that is not a map of strings is left out. */
export function parseDrafts(v: unknown): Drafts {
  const out: Drafts = {};
  if (!v || typeof v !== 'object') return out;
  for (const [k, t] of Object.entries(v as Record<string, unknown>)) {
    if (typeof t === 'string' && t.trim()) out[k] = t.slice(0, MAX_DRAFT_CHARS);
  }
  return out;
}
