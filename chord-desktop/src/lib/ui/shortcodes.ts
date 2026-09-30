// Emoji shortcodes such as :smile:. The data is Emojibase 17 (MIT). It loads on the first
// need, in its own chunk, so the main bundle stays small.

export type Shortcodes = Map<string, string>;

type Raw = Record<string, string | string[]>;

let loaded: Promise<Shortcodes> | null = null;
let ready: Shortcodes | null = null;

function fromHex(hex: string): string {
  return String.fromCodePoint(...hex.split('-').map((h) => parseInt(h, 16)));
}

/** The map from shortcode to emoji. The first call loads the data, later calls reuse it. */
export function loadShortcodes(): Promise<Shortcodes> {
  loaded ??= Promise.all([
    import('emojibase-data/en/shortcodes/iamcal.json'),
    import('emojibase-data/en/shortcodes/github.json'),
    import('emojibase-data/en/shortcodes/emojibase.json')
  ]).then((files) => {
    const map: Shortcodes = new Map();
    for (const f of files) {
      for (const [hex, names] of Object.entries(f.default as Raw)) {
        const emoji = fromHex(hex);
        for (const name of Array.isArray(names) ? names : [names]) {
          const key = name.toLowerCase();
          if (!map.has(key)) map.set(key, emoji);
        }
      }
    }
    ready = map;
    return map;
  });
  return loaded;
}

/** The map, if it is loaded already. */
export function shortcodesNow(): Shortcodes | null {
  return ready;
}

/** True when the text may hold a shortcode. It saves a load for text that has none. */
export function mayHaveShortcode(text: string): boolean {
  return /:[a-z0-9_+-]{2,}:/i.test(text);
}

const CODE = /:([a-z0-9_+-]{2,}):/gi;

/** Replace each known :shortcode: in a plain text run. An unknown one stays as it is. */
export function replaceShortcodes(text: string, map: Shortcodes): string {
  return text.replace(CODE, (all, name: string, at: number) => {
    if (at > 0 && /[A-Za-z0-9]/.test(text[at - 1])) return all;
    return map.get(name.toLowerCase()) ?? all;
  });
}

// Code in a message stays as typed: fences, double-tick spans and single-tick spans.
const CODE_SPANS = /(```[\s\S]*?```|``[\s\S]*?``|`[^`\n]+`)/;

/** Replace shortcodes in a message that the user sends. Code stays as typed. */
export function replaceShortcodesOutsideCode(text: string, map: Shortcodes): string {
  return text
    .split(CODE_SPANS)
    .map((part, i) => (i % 2 === 1 ? part : replaceShortcodes(part, map)))
    .join('');
}

/** Up to `max` shortcodes that start with `prefix`, best first, one per emoji. */
export function suggestShortcodes(
  map: Shortcodes,
  prefix: string,
  max = 8
): { name: string; emoji: string }[] {
  const q = prefix.toLowerCase();
  const found: { name: string; emoji: string }[] = [];
  const seen = new Set<string>();
  for (const [name, emoji] of map) {
    if (!name.startsWith(q) || seen.has(emoji)) continue;
    seen.add(emoji);
    found.push({ name, emoji });
  }
  found.sort((a, b) => a.name.length - b.name.length || (a.name < b.name ? -1 : 1));
  return found.slice(0, max);
}
