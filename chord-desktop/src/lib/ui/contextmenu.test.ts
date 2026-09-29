import { describe, expect, it } from 'vitest';
import { allowsNativeMenu, isMenuKey } from './contextmenu.svelte';

/** A stand-in for an element: `closest` answers true for the selectors that it lists. */
function el(matches: string[]) {
  return {
    closest: (selector: string) =>
      matches.some((m) => selector.split(',').some((s) => s.trim().startsWith(m))) ? {} : null
  };
}

describe('allowsNativeMenu', () => {
  it('keeps the webview menu in text fields', () => {
    expect(allowsNativeMenu(el(['input']))).toBe(true);
    expect(allowsNativeMenu(el(['textarea']))).toBe(true);
    expect(allowsNativeMenu(el(['[contenteditable]']))).toBe(true);
  });

  it('stops it everywhere else', () => {
    expect(allowsNativeMenu(el([]))).toBe(false);
    expect(allowsNativeMenu(null)).toBe(false);
    expect(allowsNativeMenu({})).toBe(false);
  });

  it('starts from the parent of a text node', () => {
    expect(allowsNativeMenu({ parentElement: el(['textarea']) })).toBe(true);
    expect(allowsNativeMenu({ parentElement: el([]) })).toBe(false);
  });
});

describe('isMenuKey', () => {
  const key = (k: string, mods: Partial<Record<'shiftKey' | 'ctrlKey' | 'altKey' | 'metaKey', boolean>> = {}) => ({
    key: k,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    ...mods
  });

  it('knows the two keys', () => {
    expect(isMenuKey(key('ContextMenu'))).toBe(true);
    expect(isMenuKey(key('F10', { shiftKey: true }))).toBe(true);
  });

  it('ignores other keys and chords', () => {
    expect(isMenuKey(key('F10'))).toBe(false);
    expect(isMenuKey(key('a', { shiftKey: true }))).toBe(false);
    expect(isMenuKey(key('F10', { shiftKey: true, ctrlKey: true }))).toBe(false);
  });
});
