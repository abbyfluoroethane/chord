// Checks on a link before the app opens it (BRIDGESECURITY-12, 13). The functions are pure.
//
// A masked link `[https://your-bank.com](https://evil.example)` shows one address and opens
// another. `maskedMismatch` finds it, and the app asks "Leave Chord?" with the real host.
// A host with non-Latin letters can look like a real one (a Cyrillic "a" for a Latin "a").
// `hostWarning` gives the text for the dialog: the real (Punycode) host and a warning.

const BASE = 36;
const T_MIN = 1;
const T_MAX = 26;
const SKEW = 38;
const DAMP = 700;

function adapt(delta: number, count: number, first: boolean): number {
  let d = first ? Math.floor(delta / DAMP) : delta >> 1;
  d += Math.floor(d / count);
  let k = 0;
  while (d > ((BASE - T_MIN) * T_MAX) >> 1) {
    d = Math.floor(d / (BASE - T_MIN));
    k += BASE;
  }
  return k + Math.floor(((BASE - T_MIN + 1) * d) / (d + SKEW));
}

/** Decode one Punycode label (RFC 3492) without the `xn--` prefix. Null if it is bad. */
function punyDecode(input: string): string | null {
  const out: number[] = [];
  const basic = input.lastIndexOf('-');
  for (let j = 0; j < Math.max(basic, 0); j++) {
    if (input.charCodeAt(j) >= 0x80) return null;
    out.push(input.charCodeAt(j));
  }
  let n = 128;
  let bias = 72;
  let i = 0;
  for (let idx = basic > 0 ? basic + 1 : 0; idx < input.length; ) {
    const old = i;
    for (let w = 1, k = BASE; ; k += BASE) {
      if (idx >= input.length) return null;
      const c = input.charCodeAt(idx++);
      const digit = c - 48 < 10 ? c - 22 : c - 65 < 26 ? c - 65 : c - 97 < 26 ? c - 97 : BASE;
      if (digit >= BASE) return null;
      i += digit * w;
      const t = k <= bias ? T_MIN : k >= bias + T_MAX ? T_MAX : k - bias;
      if (digit < t) break;
      w *= BASE - t;
    }
    bias = adapt(i - old, out.length + 1, old === 0);
    n += Math.floor(i / (out.length + 1));
    i %= out.length + 1;
    if (n > 0x10ffff) return null;
    out.splice(i++, 0, n);
  }
  try {
    return String.fromCodePoint(...out);
  } catch {
    return null;
  }
}

/** The Unicode form of an ASCII host: `xn--bcher-kva.example` gives `bücher.example`. */
export function toUnicodeHost(host: string): string {
  return host
    .split('.')
    .map((l) => (l.startsWith('xn--') ? (punyDecode(l.slice(4)) ?? l) : l))
    .join('.');
}

const KNOWN = ['Latin', 'Cyrillic', 'Greek', 'Armenian', 'Hebrew', 'Arabic'] as const;
const KNOWN_RE = KNOWN.map((s) => new RegExp(`\\p{Script=${s}}`, 'u'));
const OTHER_LETTER = /(?![\p{Script=Latin}\p{Script=Cyrillic}\p{Script=Greek}\p{Script=Armenian}\p{Script=Hebrew}\p{Script=Arabic}])\p{L}/u;

/** True if one label mixes letters of more than one script, like Latin with Cyrillic. */
function mixedLabel(label: string): boolean {
  let found = KNOWN_RE.filter((re) => re.test(label)).length;
  // Letters of any other script (CJK and so on) count as one more.
  if (OTHER_LETTER.test(label)) found++;
  return found > 1;
}

export type HostNote = {
  /** The host as it reads in Unicode. */
  unicode: string;
  /** A label mixes scripts. This is the usual look-alike trick. */
  mixed: boolean;
};

/** What the dialog says about an ASCII host. `null` when the host has no Punycode label. */
export function hostNote(host: string): HostNote | null {
  const unicode = toUnicodeHost(host);
  if (unicode === host) return null;
  return { unicode, mixed: unicode.split('.').some(mixedLabel) };
}

/** The warning for the dialog, or null. `host` is the ASCII host. */
export function hostWarning(host: string): string | null {
  const n = hostNote(host);
  if (!n) return null;
  return n.mixed
    ? `Warning: this name mixes letters of different scripts, so it may imitate another name. It reads "${n.unicode}".`
    : `This name has letters outside a to z. It reads "${n.unicode}". Check that it is the one you expect.`;
}

/** The host of an http or https link, in ASCII (Punycode) form. Null for any other link. */
export function linkHost(href: string): string | null {
  try {
    const u = new URL(href);
    return u.protocol === 'http:' || u.protocol === 'https:' ? u.hostname : null;
  } catch {
    return null;
  }
}

/** The host without a leading `www.`, to compare two hosts. */
function plain(host: string): string {
  return host.replace(/^www\./, '');
}

/** A URL or a host name inside link text. */
const TEXT_TARGET = /(?:https?:\/\/[^\s<>]+|(?:[\p{L}\p{N}_-]+\.)+[\p{L}]{2,}(?:\/[^\s<>]*)?)/iu;

/**
 * True if the text of a masked link looks like an address that is not the target. For
 * `[https://your-bank.com](https://evil.example)` it is true. For `[my blog](https://x.example)`
 * and for `[x.example/page](https://x.example/other)` it is false: the text names no other
 * host. Only http and https targets count.
 */
export function maskedMismatch(text: string, href: string): boolean {
  const target = linkHost(href);
  if (!target) return false;
  const m = TEXT_TARGET.exec(text);
  if (!m) return false;
  const shown = linkHost(/^https?:\/\//i.test(m[0]) ? m[0] : `http://${m[0]}`);
  return shown !== null && plain(shown) !== plain(target);
}

/** The tooltip of a masked link: the real host first, then the whole target. */
export function maskedTitle(href: string): string {
  const host = linkHost(href);
  return host ? `Opens ${host}\n${href}` : href;
}
