// Build the compact emoji text from the Emojibase list. Pure code with no app state: the
// script scripts/gen-emoji.mjs and the tests call it. The app reads the text with
// decodeEmoji (emojidata.ts). It never loads the big Emojibase JSON.
//
// The text has one line for each emoji, and an empty line between two groups. A line has
// four fields with a tab between them:
//   1. the emoji as hex code points, joined with "."
//   2. the label
//   3. the tags in lower case, joined with a space
//   4. the five skin tone variants as hex, joined with ",", or nothing
// The whole text uses one-byte letters, so it takes half the memory of raw emoji. A letter
// above U+00FF (for example the curly apostrophe) in a label or a tag becomes \uXXXX.

/** The Emojibase group numbers in picker order. Group 2 holds the skin tone swatches: not shown. */
export const GROUP_NUMBERS = [0, 1, 3, 4, 5, 6, 7, 8, 9];

/**
 * The newest Emoji version to show. The system font must draw the emoji: macOS 26 draws
 * Emoji 16. A newer emoji would show as an empty box.
 */
export const MAX_VERSION = 16;

export interface Raw {
  emoji: string;
  label: string;
  tags?: string[];
  group?: number;
  order?: number;
  version: number;
  skins?: { emoji: string; tone: number | number[] }[];
}

/** An emoji as hex code points joined with ".". */
export function toHex(emoji: string): string {
  return [...emoji].map((c) => (c.codePointAt(0) ?? 0).toString(16)).join('.');
}

/** The emoji for a hex text from toHex. */
export function fromHex(hex: string): string {
  let out = '';
  for (const cp of hex.split('.')) out += String.fromCodePoint(parseInt(cp, 16));
  return out;
}

/** Write the letters above U+00FF as \uXXXX. */
export function escapeWide(text: string): string {
  return text.replace(/[^\u0000-\u00ff]/g, (c) => '\\u' + c.charCodeAt(0).toString(16).padStart(4, '0'));
}

/** Read the text from escapeWide. */
export function unescapeWide(text: string): string {
  return text.includes('\\')
    ? text.replace(/\\u([0-9a-f]{4})/g, (_, h: string) => String.fromCharCode(parseInt(h, 16)))
    : text;
}

/** The compact text for the emoji in the picker. The groups are in picker order. */
export function compactEmoji(data: Raw[]): string {
  const byGroup = new Map<number, { line: string; order: number }[]>();
  for (const e of data) {
    if (e.group === undefined || e.group === 2 || e.version > MAX_VERSION) continue;
    const skins = e.skins
      ?.filter((s) => typeof s.tone === 'number')
      .sort((a, b) => (a.tone as number) - (b.tone as number))
      .map((s) => s.emoji);
    const tags = (e.tags ?? []).join(' ').toLowerCase();
    const line = [
      toHex(e.emoji),
      escapeWide(e.label),
      escapeWide(tags),
      skins && skins.length === 5 ? skins.map(toHex).join(',') : ''
    ].join('\t');
    const list = byGroup.get(e.group) ?? [];
    list.push({ line, order: e.order ?? 0 });
    byGroup.set(e.group, list);
  }
  return GROUP_NUMBERS.map((n) =>
    (byGroup.get(n) ?? [])
      .sort((a, b) => a.order - b.order)
      .map((x) => x.line)
      .join('\n')
  ).join('\n\n');
}
