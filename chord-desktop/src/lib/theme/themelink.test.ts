import { describe, expect, it, vi } from 'vitest';
import {
  hostOf,
  mergeFetched,
  normalizeThemeUrl,
  readLibrary,
  refreshLinked,
  type CustomTheme
} from './themelink';
import { MAX_THEME_CHARS } from './themecss';

describe('normalizeThemeUrl', () => {
  it('turns a GitHub page link into a raw link', () => {
    expect(normalizeThemeUrl('https://github.com/u/r/blob/main/themes/a.css')).toEqual({
      ok: true,
      url: 'https://raw.githubusercontent.com/u/r/main/themes/a.css'
    });
    expect(normalizeThemeUrl('https://github.com/u/r/raw/v1.2/a.css')).toEqual({
      ok: true,
      url: 'https://raw.githubusercontent.com/u/r/v1.2/a.css'
    });
  });

  it('keeps other https hosts', () => {
    for (const u of [
      'https://raw.githubusercontent.com/u/r/main/a.css',
      'https://gist.githubusercontent.com/u/abc/raw/def/a.css',
      'https://cdn.jsdelivr.net/gh/u/r@main/a.css',
      'https://example.org/a.css?v=2'
    ]) {
      expect(normalizeThemeUrl(u)).toEqual({ ok: true, url: u });
    }
  });

  it('rejects a link that is not https', () => {
    for (const u of [
      'http://example.org/a.css',
      'file:///a.css',
      'javascript:alert(1)',
      'a.css',
      ''
    ]) {
      expect(normalizeThemeUrl(u).ok).toBe(false);
    }
    expect(normalizeThemeUrl('https://u:p@example.org/a.css').ok).toBe(false);
  });

  it('drops the fragment and trims', () => {
    expect(normalizeThemeUrl('  https://example.org/a.css#top ')).toEqual({
      ok: true,
      url: 'https://example.org/a.css'
    });
  });
});

describe('hostOf', () => {
  it('returns the host', () => {
    expect(hostOf('https://www.example.org/a.css')).toBe('example.org');
    expect(hostOf('nonsense')).toBe('nonsense');
  });
});

const lib = (): CustomTheme[] => [
  { id: 'a', css: 'old', url: 'https://example.org/a.css', updated: 1 },
  { id: 'b', css: 'pasted' }
];

describe('mergeFetched', () => {
  it('replaces the CSS of a linked theme', () => {
    const next = mergeFetched(lib(), 'a', ' new ', 9);
    expect(next[0]).toEqual({ id: 'a', css: 'new', url: 'https://example.org/a.css', updated: 9 });
    expect(next[1]).toEqual({ id: 'b', css: 'pasted' });
  });

  it('keeps the old CSS for an empty or a big file', () => {
    const l = lib();
    expect(mergeFetched(l, 'a', '   ', 9)).toBe(l);
    expect(mergeFetched(l, 'a', 'x'.repeat(MAX_THEME_CHARS + 1), 9)).toBe(l);
  });

  it('ignores a pasted theme and an unknown id', () => {
    const l = lib();
    expect(mergeFetched(l, 'b', 'new', 9)).toBe(l);
    expect(mergeFetched(l, 'zzz', 'new', 9)).toBe(l);
  });

  it('does not change a theme when the CSS is the same', () => {
    const l = lib();
    expect(mergeFetched(l, 'a', 'old', 9)).toBe(l);
  });
});

describe('readLibrary', () => {
  it('reads old records and new records', () => {
    expect(
      readLibrary([
        { id: 'a', css: 'x' },
        { id: 'b', css: 'y', url: 'https://e.org/b.css', updated: 5 },
        { id: 3, css: 'z' },
        null
      ])
    ).toEqual([
      { id: 'a', css: 'x' },
      { id: 'b', css: 'y', url: 'https://e.org/b.css', updated: 5 }
    ]);
    expect(readLibrary('nope')).toEqual([]);
  });
});

describe('refreshLinked', () => {
  it('updates the linked themes and logs a failure', async () => {
    const l: CustomTheme[] = [
      { id: 'a', css: 'old', url: 'https://e.org/a.css' },
      { id: 'b', css: 'old', url: 'https://e.org/b.css' },
      { id: 'c', css: 'pasted' }
    ];
    const fetcher = vi.fn(async (u: string) => {
      if (u.endsWith('b.css')) throw new Error('offline');
      return 'fresh';
    });
    const applied: [string, string][] = [];
    const log = vi.fn();
    await refreshLinked(l, fetcher, (id, css) => applied.push([id, css]), log);
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(applied).toEqual([['a', 'fresh']]);
    expect(log).toHaveBeenCalledOnce();
  });
});
