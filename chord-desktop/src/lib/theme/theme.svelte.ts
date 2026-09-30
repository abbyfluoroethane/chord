// Themes. The user picks a mode (dark, light, or the system setting), one theme for dark
// and one for light, and an accent for each theme. The active theme's CSS goes into one
// style element after tokens.css. A switch of the system setting changes the theme at
// once.
//
// The library holds the built-in themes and the themes that the user pasted in. A theme
// is plain CSS with a comment header: see themecss.ts.

import catppuccinLatte from './themes/catppuccin-latte.css?raw';
import catppuccinMocha from './themes/catppuccin-mocha.css?raw';
import chordDark from './themes/chord-dark.css?raw';
import chordLight from './themes/chord-light.css?raw';
import { MAX_THEME_CHARS, parseTheme, type ThemeInfo, type ThemeMode } from './themecss';

export type ThemeChoice = 'system' | 'dark' | 'light';

export interface Theme {
  id: string;
  css: string;
  info: ThemeInfo;
  builtIn: boolean;
}

const KEY = 'chord.theme';
const LIBRARY_KEY = 'chord.themes';
const PICK_KEY = 'chord.themePick';

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

class ThemeStore {
  choice = $state<ThemeChoice>('system');
  /** The imported themes, as the user pasted them. */
  custom = $state<{ id: string; css: string }[]>([]);
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
    const custom = read<{ id: string; css: string }[]>(LIBRARY_KEY);
    if (Array.isArray(custom)) {
      this.custom = custom.filter((t) => typeof t?.id === 'string' && typeof t?.css === 'string');
    }
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
  }
}

export const theme = new ThemeStore();
