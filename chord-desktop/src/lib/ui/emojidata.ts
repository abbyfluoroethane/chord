// Emoji data for the picker: Emojibase 17 (MIT), English labels and tags. The file is big,
// so it loads on the first open of the picker, in its own chunk.

export interface EmojiEntry {
  /** The emoji with the default skin tone. */
  emoji: string;
  label: string;
  /** Lower-case words to search: the label and the tags. */
  words: string;
  /** The emoji in skin tones 1 to 5, if it has them. */
  skins: string[] | null;
}

export interface EmojiGroup {
  id: string;
  label: string;
  emoji: EmojiEntry[];
}

/** The Emojibase groups in picker order. Group 2 holds the skin tone swatches: not shown. */
const GROUPS: { n: number; id: string; label: string }[] = [
  { n: 0, id: 'smileys', label: 'Smileys and emotion' },
  { n: 1, id: 'people', label: 'People and body' },
  { n: 3, id: 'nature', label: 'Animals and nature' },
  { n: 4, id: 'food', label: 'Food and drink' },
  { n: 5, id: 'travel', label: 'Travel and places' },
  { n: 6, id: 'activities', label: 'Activities' },
  { n: 7, id: 'objects', label: 'Objects' },
  { n: 8, id: 'symbols', label: 'Symbols' },
  { n: 9, id: 'flags', label: 'Flags' }
];

/**
 * The newest Emoji version to show. The system font must draw the emoji: macOS 26 draws
 * Emoji 16. A newer emoji would show as an empty box.
 */
const MAX_VERSION = 16;

interface Raw {
  emoji: string;
  label: string;
  tags?: string[];
  group?: number;
  order?: number;
  version: number;
  skins?: { emoji: string; tone: number | number[] }[];
}

let loaded: Promise<EmojiGroup[]> | null = null;

/** The emoji groups. The first call loads the data, later calls reuse it. */
export function loadEmoji(): Promise<EmojiGroup[]> {
  loaded ??= import('emojibase-data/en/data.json').then(({ default: data }) => {
    const byGroup = new Map<number, (EmojiEntry & { order: number })[]>();
    for (const e of data as Raw[]) {
      if (e.group === undefined || e.group === 2 || e.version > MAX_VERSION) continue;
      const skins = e.skins
        ?.filter((s) => typeof s.tone === 'number')
        .sort((a, b) => (a.tone as number) - (b.tone as number))
        .map((s) => s.emoji);
      const list = byGroup.get(e.group) ?? [];
      list.push({
        emoji: e.emoji,
        label: e.label,
        words: [e.label, ...(e.tags ?? [])].join(' ').toLowerCase(),
        skins: skins && skins.length === 5 ? skins : null,
        order: e.order ?? 0
      });
      byGroup.set(e.group, list);
    }
    return GROUPS.map((g) => ({
      id: g.id,
      label: g.label,
      emoji: (byGroup.get(g.n) ?? []).sort((a, b) => a.order - b.order)
    }));
  });
  return loaded;
}

/** The emoji that match a search, best first: a word that starts with the text first. */
export function searchEmoji(groups: EmojiGroup[], text: string, max = 80): EmojiEntry[] {
  const q = text.trim().toLowerCase();
  if (!q) return [];
  const starts: EmojiEntry[] = [];
  const contains: EmojiEntry[] = [];
  for (const g of groups) {
    for (const e of g.emoji) {
      const at = e.words.indexOf(q);
      if (at < 0) continue;
      if (at === 0 || e.words[at - 1] === ' ') starts.push(e);
      else contains.push(e);
    }
  }
  return [...starts, ...contains].slice(0, max);
}

/** The emoji in skin tone `tone` (0 is the default yellow). */
export function withTone(e: EmojiEntry, tone: number): string {
  return tone > 0 && e.skins ? e.skins[tone - 1] : e.emoji;
}
