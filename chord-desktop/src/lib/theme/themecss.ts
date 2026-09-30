// Reads a theme: CSS with a comment header, as on Vencord and BetterDiscord.
//
//   /**
//    * @name My theme
//    * @author Me
//    * @mode dark            (or light; without it, Chord reads --surface-100)
//    * @default blue         (the first accent to use)
//    * @accent blue Blue     (one line for each accent: id, then a name)
//    */
//   :root { --surface-100: #1e1e2e; --brand: #89b4fa; ... }
//   :root[data-accent="blue"] { --brand: #89b4fa; }
//
// An accent is a [data-accent="id"] rule. Chord puts the id on the root element. The
// rest of the CSS is free: a theme can style any part of the app.

export type ThemeMode = 'dark' | 'light';

export interface ThemeAccent {
  id: string;
  name: string;
  /** The --brand colour of the accent, for its swatch. */
  color: string | null;
}

export interface ThemeInfo {
  name: string;
  author: string | null;
  description: string | null;
  mode: ThemeMode;
  accents: ThemeAccent[];
  /** The accent to use before the user picks one. */
  defaultAccent: string | null;
  /** Colours for the preview card. */
  swatch: { surface: string; panel: string; ink: string; brand: string };
}

/** The most characters of an imported theme: 200 000. */
export const MAX_THEME_CHARS = 200_000;

function header(css: string): Map<string, string[]> {
  const out = new Map<string, string[]>();
  const block = css.match(/\/\*\*?([\s\S]*?)\*\//);
  if (!block) return out;
  for (const raw of block[1].split('\n')) {
    const m = raw.replace(/^\s*\*?\s?/, '').match(/^@([\w-]+)\s+(.+?)\s*$/);
    if (!m) continue;
    const list = out.get(m[1].toLowerCase()) ?? [];
    list.push(m[2]);
    out.set(m[1].toLowerCase(), list);
  }
  return out;
}

/** The value of a custom property in the first plain `:root { }` rule. */
function rootVar(css: string, name: string): string | null {
  const root = css.match(/(?:^|[}\s])(?::root|html)\s*\{([^}]*)\}/);
  if (!root) return null;
  const m = root[1].match(new RegExp(`${name}\\s*:\\s*([^;]+);`));
  return m ? m[1].trim() : null;
}

/** 0 (black) to 1 (white) for a #rgb or #rrggbb colour, or null. */
export function luminance(color: string | null): number | null {
  const m = color?.trim().match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i);
  if (!m) return null;
  const hex = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1];
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

const title = (id: string) => id.replace(/[-_]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());

export function parseTheme(css: string): ThemeInfo {
  const meta = header(css);
  const one = (k: string) => meta.get(k)?.[0] ?? null;

  const names = new Map<string, string>();
  for (const line of meta.get('accent') ?? []) {
    const [id, ...rest] = line.split(/\s+/);
    if (id) names.set(id, rest.join(' ') || title(id));
  }
  const accents: ThemeAccent[] = [];
  const seen = new Set<string>();
  const rule = /\[data-accent\s*=\s*["']?([\w-]+)["']?\]\s*\{([^}]*)\}/g;
  for (const m of css.matchAll(rule)) {
    const id = m[1];
    const brand = m[2].match(/--brand\s*:\s*([^;]+);/)?.[1].trim() ?? null;
    if (seen.has(id)) {
      const a = accents.find((x) => x.id === id);
      if (a && !a.color && brand) a.color = brand;
      continue;
    }
    seen.add(id);
    accents.push({ id, name: names.get(id) ?? title(id), color: brand });
  }
  // An accent in the header only, with no rule, uses the theme's own --brand.
  for (const [id, name] of names) {
    if (!seen.has(id)) accents.push({ id, name, color: rootVar(css, '--brand') });
  }
  // The header order wins over the rule order: it is the order the author chose.
  const order = [...names.keys()];
  accents.sort((a, b) => {
    const i = order.indexOf(a.id);
    const j = order.indexOf(b.id);
    return (i < 0 ? order.length : i) - (j < 0 ? order.length : j);
  });

  const surface = rootVar(css, '--surface-100') ?? '#111316';
  const declared = one('mode')?.toLowerCase();
  const mode: ThemeMode =
    declared === 'light' || declared === 'dark'
      ? declared
      : (luminance(surface) ?? 0) > 0.5
        ? 'light'
        : 'dark';
  const wanted = one('default');
  return {
    name: one('name') ?? 'Imported theme',
    author: one('author'),
    description: one('description'),
    mode,
    accents,
    defaultAccent: accents.find((a) => a.id === wanted)?.id ?? accents[0]?.id ?? null,
    swatch: {
      surface,
      panel: rootVar(css, '--surface-200') ?? surface,
      ink: rootVar(css, '--ink') ?? (mode === 'light' ? '#1a1c20' : '#ece8e1'),
      brand: accents.find((a) => a.id === wanted)?.color ?? rootVar(css, '--brand') ?? '#f2a93b'
    }
  };
}
