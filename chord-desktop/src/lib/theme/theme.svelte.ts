// Themes. The user picks a mode (dark, light, or the system setting), one theme for dark
// and one for light, and an accent for each theme. The active theme's CSS goes into one
// style element after tokens.css. A switch of the system setting changes the theme at
// once.
//
// The library holds the built-in themes and the themes that the user pasted in or linked.
// A theme is plain CSS with a comment header: see themecss.ts. A linked theme keeps its
// URL. Chord fetches it again at each launch and keeps the old CSS if the fetch fails.

import catppuccinLatte from './themes/catppuccin-latte.css?raw';
import catppuccinMocha from './themes/catppuccin-mocha.css?raw';
import chordDark from './themes/chord-dark.css?raw';
import chordLight from './themes/chord-light.css?raw';
import { api, live } from '$lib/ui/bridge';
import { MAX_THEME_CHARS, parseTheme, type ThemeInfo, type ThemeMode } from './themecss';
import {
  checkFetched,
  mergeFetched,
  normalizeThemeUrl,
  readLibrary,
  refreshLinked,
  type CustomTheme
} from './themelink';

export type ThemeChoice = 'system' | 'dark' | 'light';

export interface Theme {
  id: string;
  css: string;
  info: ThemeInfo;
  builtIn: boolean;
  /** The link of a linked theme. */
  url?: string;
}

const KEY = 'chord.theme';
const LIBRARY_KEY = 'chord.themes';
const PICK_KEY = 'chord.themePick';
/** Read by the inline splash in src/app.html. */
const SPLASH_KEY = 'chord.splash';

const BUILT_IN: Theme[] = [
  ['chord-dark', chordDark],
  ['chord-light', chordLight],
  ['catppuccin-mocha', catppuccinMocha],
  ['catppuccin-latte', catppuccinLatte]
].map(([id, css]) => ({ id, css, info: parseTheme(css), builtIn: true }));

interface Pick {
  dark: string;
  light: string;
  /** The accent id for each theme id. */
  accents: Record<string, string>;
}

function read<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage blocked or full: the change lasts for this session */
  }
}

/** Get the CSS of a link. The app does it in Rust. The static preview tries the browser. */
async function fetchThemeCss(url: string): Promise<string> {
  if (live) return (await api()).themeFetch(url);
  const r = await fetch(url, { credentials: 'omit' });
  if (!r.ok) throw new Error(`the server answered ${r.status}`);
  return await r.text();
}

/** The text for the user when a fetch fails. */
function fetchMessage(e: unknown): string {
  const raw =
    typeof e === 'string' ? e : ((e as { message?: string } | null)?.message ?? String(e));
  return live
    ? `Chord could not load this link: ${raw}`
    : 'The browser could not load this link. The site may not allow other sites to read it. Use the app, or paste the CSS.';
}

class ThemeStore {
  choice = $state<ThemeChoice>('system');
  /** The imported themes: pasted CSS, or CSS from a link. */
  custom = $state<CustomTheme[]>([]);
  pick = $state<Pick>({ dark: 'chord-dark', light: 'chord-light', accents: {} });
  /** The system setting, for the "system" choice. */
  systemLight = $state(false);

  /** Every theme: the built-in ones first. */
  library = $derived<Theme[]>([
    ...BUILT_IN,
    ...this.custom.map((t) => ({ ...t, info: parseTheme(t.css), builtIn: false }))
  ]);

  /** The mode that shows now. */
  mode = $derived<ThemeMode>(
    this.choice === 'system' ? (this.systemLight ? 'light' : 'dark') : this.choice
  );

  private styleEl: HTMLStyleElement | null = null;

  load() {
    try {
      const v = localStorage.getItem(KEY);
      if (v === 'dark' || v === 'light' || v === 'system') this.choice = v;
    } catch {
      /* storage blocked, keep the default */
    }
    this.custom = readLibrary(read<unknown>(LIBRARY_KEY));
    const pick = read<Partial<Pick>>(PICK_KEY);
    if (pick) {
      this.pick = {
        dark: typeof pick.dark === 'string' ? pick.dark : 'chord-dark',
        light: typeof pick.light === 'string' ? pick.light : 'chord-light',
        accents: pick.accents && typeof pick.accents === 'object' ? pick.accents : {}
      };
    }
    const query = window.matchMedia('(prefers-color-scheme: light)');
    this.systemLight = query.matches;
    query.addEventListener('change', (e) => {
      this.systemLight = e.matches;
      this.apply();
    });
    this.apply();
    // The stored CSS is on the page now. The update runs after the first paint.
    void this.refresh();
  }

  set(choice: ThemeChoice) {
    this.choice = choice;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      /* ignore */
    }
    this.apply();
  }

  /** The theme for a mode. A missing theme falls back to Chord. */
  themeFor(mode: ThemeMode): Theme {
    const id = mode === 'dark' ? this.pick.dark : this.pick.light;
    return (
      this.library.find((t) => t.id === id && t.info.mode === mode) ??
      BUILT_IN.find((t) => t.id === (mode === 'dark' ? 'chord-dark' : 'chord-light'))!
    );
  }

  /** The accent of a theme: the user's pick, else the theme's default. */
  accentOf(t: Theme): string | null {
    const picked = this.pick.accents[t.id];
    return t.info.accents.some((a) => a.id === picked) ? picked : t.info.defaultAccent;
  }

  /** Use a theme for its mode. */
  use(id: string) {
    const t = this.library.find((x) => x.id === id);
    if (!t) return;
    this.pick = { ...this.pick, [t.info.mode]: id };
    write(PICK_KEY, this.pick);
    this.apply();
  }

  setAccent(themeId: string, accent: string) {
    this.pick = { ...this.pick, accents: { ...this.pick.accents, [themeId]: accent } };
    write(PICK_KEY, this.pick);
    this.apply();
  }

  /**
   * Add a pasted theme to the library and use it. Returns the theme, or an error for
   * the user.
   */
  import(css: string): { ok: true; theme: Theme } | { ok: false; error: string } {
    const text = css.trim();
    if (!text) return { ok: false, error: 'Paste the CSS of a theme.' };
    if (text.length > MAX_THEME_CHARS) {
      return { ok: false, error: 'This theme is too big. The limit is 200 000 characters.' };
    }
    const id = `custom-${Date.now().toString(36)}`;
    this.custom = [...this.custom, { id, css: text }];
    write(LIBRARY_KEY, this.custom);
    this.use(id);
    return { ok: true, theme: this.library.find((t) => t.id === id)! };
  }

  /**
   * Add a theme from a link and use it. A theme with the same link gets the new CSS
   * instead of a second card.
   */
  async importLink(
    input: string
  ): Promise<{ ok: true; theme: Theme } | { ok: false; error: string }> {
    const u = normalizeThemeUrl(input);
    if (!u.ok) return u;
    let fetched: string;
    try {
      fetched = await fetchThemeCss(u.url);
    } catch (e) {
      return { ok: false, error: fetchMessage(e) };
    }
    const checked = checkFetched(fetched);
    if (!checked.ok) return checked;
    const known = this.custom.find((t) => t.url === u.url);
    const id = known?.id ?? `custom-${Date.now().toString(36)}`;
    const item: CustomTheme = { id, css: checked.css, url: u.url, updated: Date.now() };
    this.custom = known
      ? this.custom.map((t) => (t.id === id ? item : t))
      : [...this.custom, item];
    write(LIBRARY_KEY, this.custom);
    this.use(id);
    return { ok: true, theme: this.library.find((t) => t.id === id)! };
  }

  /** Fetch each linked theme again. A failure keeps the old CSS and only writes a log. */
  async refresh() {
    await refreshLinked(
      this.custom,
      fetchThemeCss,
      (id, css) => {
        const next = mergeFetched(this.custom, id, css, Date.now());
        if (next === this.custom) return;
        this.custom = next;
        write(LIBRARY_KEY, this.custom);
        // The active theme changes at once.
        if (this.themeFor(this.mode).id === id) this.apply();
      },
      (message) => console.warn(message)
    );
  }

  /** Remove an imported theme. A slot that used it goes back to Chord. */
  remove(id: string) {
    this.custom = this.custom.filter((t) => t.id !== id);
    write(LIBRARY_KEY, this.custom);
    const accents = { ...this.pick.accents };
    delete accents[id];
    this.pick = {
      dark: this.pick.dark === id ? 'chord-dark' : this.pick.dark,
      light: this.pick.light === id ? 'chord-light' : this.pick.light,
      accents
    };
    write(PICK_KEY, this.pick);
    this.apply();
  }

  /** Put the active theme on the page. */
  private apply() {
    const root = document.documentElement;
    const t = this.themeFor(this.mode);
    if (!this.styleEl) {
      this.styleEl = document.createElement('style');
      this.styleEl.id = 'chord-theme';
      document.head.append(this.styleEl);
    }
    this.styleEl.textContent = t.css;
    root.dataset.mode = this.mode;
    root.dataset.themeId = t.id;
    const accent = this.accentOf(t);
    if (accent) root.dataset.accent = accent;
    else delete root.dataset.accent;
    this.saveSplash();
  }

  /**
   * Keep the background and text colour of each mode for the first paint. The inline
   * splash in app.html reads them before any script loads, so the window opens in the
   * user's theme and not in the colours of the system setting.
   */
  private saveSplash() {
    const cs = getComputedStyle(document.documentElement);
    const saved = read<Record<string, unknown>>(SPLASH_KEY) ?? {};
    saved.choice = this.choice;
    saved[this.mode] = {
      bg: cs.getPropertyValue('--surface-100').trim(),
      ink: cs.getPropertyValue('--ink').trim()
    };
    write(SPLASH_KEY, saved);
  }
}

export const theme = new ThemeStore();
