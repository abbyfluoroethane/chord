// Emoji data for the picker: Emojibase 17 (MIT), English labels and tags. The data is a
// compact text of about 150 KB (emojidata.gen.ts, made by scripts/gen-emoji.mjs). It has
// its own chunk. It loads once, in idle time soon after the app starts (see preloadEmoji),
// and the groups stay in memory. The picker then opens with no wait. The search words are
// not stored: a search builds them for each emoji when it runs.
import { compactEmoji, fromHex, unescapeWide, type Raw } from './emojibuild';

export type { Raw };

export interface EmojiEntry {
  /** The emoji with the default skin tone. */
  emoji: string;
  label: string;
  /** The tags in lower case, joined with a space. The search reads them with the label. */
  tags: string;
  /** The emoji in skin tones 1 to 5, if it has them. */
  skins: string[] | null;
}

export interface EmojiGroup {
  id: string;
  label: string;
  emoji: EmojiEntry[];
}

/** The groups in picker order, as in GROUP_NUMBERS (emojibuild.ts). */
const GROUPS: { id: string; label: string }[] = [
  { id: 'smileys', label: 'Smileys and emotion' },
  { id: 'people', label: 'People and body' },
  { id: 'nature', label: 'Animals and nature' },
  { id: 'food', label: 'Food and drink' },
  { id: 'travel', label: 'Travel and places' },
  { id: 'activities', label: 'Activities' },
  { id: 'objects', label: 'Objects' },
  { id: 'symbols', label: 'Symbols' },
  { id: 'flags', label: 'Flags' }
];

let loaded: Promise<EmojiGroup[]> | null = null;
let ready: EmojiGroup[] | null = null;
let byChar: Map<string, EmojiEntry> | null = null;

/** Read the compact text (see emojibuild.ts) into the picker groups. */
export function decodeEmoji(text: string): EmojiGroup[] {
  const blocks = text.split('\n\n');
  return GROUPS.map((g, n) => ({
    id: g.id,
    label: g.label,
    emoji: (blocks[n] ?? '')
      .split('\n')
      .filter(Boolean)
      .map((line): EmojiEntry => {
        const [hex, label, tags, skins] = line.split('\t');
        return {
          emoji: fromHex(hex),
          label: unescapeWide(label),
          tags: unescapeWide(tags),
          skins: skins ? skins.split(',').map(fromHex) : null
        };
      })
  }));
}

/** Build the picker groups from the Emojibase list. The tests call it. */
export function buildGroups(data: Raw[]): EmojiGroup[] {
  return decodeEmoji(compactEmoji(data));
}

/** The emoji groups. The first call loads the data, later calls reuse it. */
export function loadEmoji(): Promise<EmojiGroup[]> {
  loaded ??= import('./emojidata.gen').then(({ default: text }) => {
    ready = decodeEmoji(text);
    return ready;
  });
  return loaded;
}

/** The emoji groups if they are loaded already, or null. Never waits. */
export function emojiNow(): EmojiGroup[] | null {
  return ready;
}

/** Load the data in idle time, so that the first open of the picker is quick. */
export function preloadEmoji(): void {
  if (loaded || typeof window === 'undefined') return;
  const run = () => void loadEmoji().catch(() => (loaded = null));
  if ('requestIdleCallback' in window) window.requestIdleCallback(run, { timeout: 4000 });
  else setTimeout(run, 1500);
}

/** One emoji entry by its default character, or undefined. Built once. */
export function emojiEntry(emoji: string): EmojiEntry | undefined {
  if (!byChar && ready) {
    byChar = new Map(ready.flatMap((g) => g.emoji.map((e) => [e.emoji, e] as const)));
  }
  return byChar?.get(emoji);
}

/** The emoji that match a search, best first: a word that starts with the text first. */
export function searchEmoji(groups: EmojiGroup[], text: string, max = 80): EmojiEntry[] {
  const q = text.trim().toLowerCase();
  if (!q) return [];
  const starts: EmojiEntry[] = [];
  const contains: EmojiEntry[] = [];
  for (const g of groups) {
    for (const e of g.emoji) {
      const words = `${e.label.toLowerCase()} ${e.tags}`;
      const at = words.indexOf(q);
      if (at < 0) continue;
      if (at === 0 || words[at - 1] === ' ') {
        starts.push(e);
        if (starts.length >= max) return starts;
      } else contains.push(e);
    }
  }
  return [...starts, ...contains].slice(0, max);
}

/** The emoji in skin tone `tone` (0 is the default yellow). */
export function withTone(e: EmojiEntry, tone: number): string {
  return tone > 0 && e.skins ? e.skins[tone - 1] : e.emoji;
}
