// Themes from a link: the URL rules, and the update of the stored library. This file has
// no state and no browser calls, so the tests can run it as it is.

import { MAX_THEME_CHARS } from './themecss';

/** An imported theme as stored. `url` and `updated` are set for a linked theme only. */
export interface CustomTheme {
  id: string;
  css: string;
  /** The https URL that Chord fetches on each launch. */
  url?: string;
  /** When Chord last stored CSS from `url`, in ms since 1970. */
  updated?: number;
  /**
   * CSS that the link gave after the user added the theme. It waits for the user: a linked
   * theme never changes the look or the requests of the app without a yes
   * (BRIDGESECURITY-05).
   */
  pending?: string;
}

export type UrlResult = { ok: true; url: string } | { ok: false; error: string };

const GITHUB_BLOB = /^\/([^/]+)\/([^/]+)\/(?:blob|raw)\/(.+)$/;

/**
 * Check the link of a theme. Only https is fine. A GitHub page link turns into the
 * raw link of the same file. Other hosts stay as they are.
 */
export function normalizeThemeUrl(input: string): UrlResult {
  const text = input.trim();
  if (!text) return { ok: false, error: 'Paste the link of a theme.' };
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return { ok: false, error: 'This is not a link. It must start with https://.' };
  }
  if (url.protocol !== 'https:') {
    return { ok: false, error: 'The link must start with https://.' };
  }
  if (url.username || url.password) {
    return { ok: false, error: 'The link cannot have a user name or a password.' };
  }
  const host = url.hostname.toLowerCase();
  if (host === 'github.com' || host === 'www.github.com') {
    const m = GITHUB_BLOB.exec(url.pathname);
    if (m) {
      return { ok: true, url: `https://raw.githubusercontent.com/${m[1]}/${m[2]}/${m[3]}` };
    }
  }
  url.hash = '';
  return { ok: true, url: url.toString() };
}

/** The host of a link, for the theme card. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '');
  } catch {
    return url;
  }
}

/** The CSS of a fetch, trimmed. An error text if the CSS is not fit for a theme. */
export function checkFetched(
  css: string
): { ok: true; css: string } | { ok: false; error: string } {
  const text = css.trim();
  if (!text) return { ok: false, error: 'The link gave an empty file.' };
  if (text.length > MAX_THEME_CHARS) {
    return { ok: false, error: 'This theme is too big. The limit is 200 000 characters.' };
  }
  return { ok: true, css: text };
}

/**
 * Put fetched CSS in the library as an update that waits for the user (`pending`). The
 * applied CSS stays as it is. A theme that is not there, or that has no link, stays as
 * it is. Invalid CSS changes nothing. The same CSS as the applied one clears an old
 * update. The function returns the same array when nothing changes.
 */
export function mergeFetched(
  library: CustomTheme[],
  id: string,
  fetched: string,
  now: number
): CustomTheme[] {
  const checked = checkFetched(fetched);
  const at = library.findIndex((t) => t.id === id);
  if (!checked.ok || at < 0 || !library[at].url) return library;
  const theme = library[at];
  const next = library.slice();
  if (theme.css === checked.css) {
    if (theme.pending === undefined) return library;
    const { pending: _dropped, ...rest } = theme;
    next[at] = rest;
    return next;
  }
  if (theme.pending === checked.css) return library;
  next[at] = { ...theme, pending: checked.css };
  return next;
}

/** Apply the waiting update of a theme. */
export function acceptPending(library: CustomTheme[], id: string, now: number): CustomTheme[] {
  const at = library.findIndex((t) => t.id === id);
  if (at < 0 || library[at].pending === undefined) return library;
  const { pending, ...rest } = library[at];
  const next = library.slice();
  next[at] = { ...rest, css: pending!, updated: now };
  return next;
}

/** Throw away the waiting update of a theme. The next launch can offer it again. */
export function dismissPending(library: CustomTheme[], id: string): CustomTheme[] {
  const at = library.findIndex((t) => t.id === id);
  if (at < 0 || library[at].pending === undefined) return library;
  const { pending: _dropped, ...rest } = library[at];
  const next = library.slice();
  next[at] = rest;
  return next;
}

/** How many lines an update adds and removes, for the question to the user. */
export function lineChange(oldCss: string, newCss: string): { added: number; removed: number } {
  const count = (text: string) => {
    const m = new Map<string, number>();
    for (const l of text.split('\n')) {
      const k = l.trim();
      if (k) m.set(k, (m.get(k) ?? 0) + 1);
    }
    return m;
  };
  const a = count(oldCss);
  const b = count(newCss);
  let added = 0;
  let removed = 0;
  for (const [k, n] of b) added += Math.max(0, n - (a.get(k) ?? 0));
  for (const [k, n] of a) removed += Math.max(0, n - (b.get(k) ?? 0));
  return { added, removed };
}

/** Read the stored library. A record with a bad field is dropped. */
export function readLibrary(raw: unknown): CustomTheme[] {
  if (!Array.isArray(raw)) return [];
  const out: CustomTheme[] = [];
  for (const t of raw) {
    if (typeof t?.id !== 'string' || typeof t?.css !== 'string') continue;
    const item: CustomTheme = { id: t.id, css: t.css };
    if (typeof t.url === 'string' && t.url) item.url = t.url;
    if (typeof t.updated === 'number') item.updated = t.updated;
    if (typeof t.pending === 'string' && t.url) item.pending = t.pending;
    out.push(item);
  }
  return out;
}

/**
 * Fetch every linked theme at once. `apply` gets each answer as it arrives, so the first
 * answer does not wait for the slowest one. A failed fetch keeps the old CSS. It goes
 * to `log` only.
 */
export async function refreshLinked(
  library: CustomTheme[],
  fetcher: (url: string) => Promise<string>,
  apply: (id: string, css: string) => void,
  log: (message: string) => void = () => {}
): Promise<void> {
  await Promise.all(
    library
      .filter((t) => t.url)
      .map(async (t) => {
        try {
          apply(t.id, await fetcher(t.url!));
        } catch (e) {
          log(`theme update failed for ${t.url}: ${e instanceof Error ? e.message : String(e)}`);
        }
      })
  );
}
