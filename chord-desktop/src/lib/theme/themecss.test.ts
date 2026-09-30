import { describe, expect, it } from 'vitest';
// @ts-expect-error type error without @types/node package, as in vite.config.js
import { readFileSync } from 'node:fs';
import { parseTheme } from './themecss';

// The test runner turns a CSS import into an empty string, so read the files.
const read = (name: string) => readFileSync(new URL(`./themes/${name}.css`, import.meta.url), 'utf8');
const mocha = read('catppuccin-mocha');
const latte = read('catppuccin-latte');
const chordDark = read('chord-dark');

describe('parseTheme', () => {
  it('reads the built-in themes', () => {
    const m = parseTheme(mocha);
    expect(m.name).toBe('Catppuccin Mocha');
    expect(m.mode).toBe('dark');
    expect(m.accents).toHaveLength(14);
    expect(m.defaultAccent).toBe('mauve');
    expect(m.accents.find((a) => a.id === 'blue')?.color).toBe('#89b4fa');
    expect(parseTheme(latte).mode).toBe('light');
    const c = parseTheme(chordDark);
    expect(c.accents).toEqual([{ id: 'amber', name: 'Amber', color: '#f2a93b' }]);
    expect(c.swatch.surface).toBe('#111316');
  });

  it('guesses the mode from the surface colour, and names accents from their ids', () => {
    const t = parseTheme(`
      /* @name Paper */
      :root { --surface-100: #fafafa; --brand: #d33; }
      :root[data-accent="sea-green"] { --brand: #2a8; }
      html[data-accent='dusk'] { --brand: #535; }
    `);
    expect(t.mode).toBe('light');
    expect(t.accents.map((a) => a.name)).toEqual(['Sea Green', 'Dusk']);
    expect(t.defaultAccent).toBe('sea-green');
  });

  it('keeps the header order of the accents', () => {
    const t = parseTheme(`
      /**
       * @accent iris Iris
       * @accent rose Rose
       */
      :root { --surface-100: #191724; }
      :root[data-accent="rose"] { --brand: #ebbcba; }
      :root[data-accent="iris"] { --brand: #c4a7e7; }
    `);
    expect(t.accents.map((a) => a.id)).toEqual(['iris', 'rose']);
    expect(t.defaultAccent).toBe('iris');
  });

  it('gives a name and dark mode to a bare theme', () => {
    const t = parseTheme('.message { color: red; }');
    expect(t.name).toBe('Imported theme');
    expect(t.mode).toBe('dark');
    expect(t.accents).toEqual([]);
  });
});
