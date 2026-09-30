import { describe, expect, it } from 'vitest';
import {
  loadShortcodes,
  replaceShortcodes,
  replaceShortcodesOutsideCode,
  suggestShortcodes
} from './shortcodes';

const codes = new Map([
  ['smile', '😄'],
  ['smiley', '😃'],
  ['tada', '🎉']
]);

describe('replaceShortcodesOutsideCode', () => {
  it('replaces known shortcodes', () => {
    expect(replaceShortcodesOutsideCode('hi :smile: and :TADA:', codes)).toBe('hi 😄 and 🎉');
  });
  it('keeps unknown shortcodes', () => {
    expect(replaceShortcodesOutsideCode(':nope: 12:30:45', codes)).toBe(':nope: 12:30:45');
  });
  it('keeps code as typed', () => {
    const body = ':smile: `:smile:` ```\n:smile:\n``` :smile:';
    expect(replaceShortcodesOutsideCode(body, codes)).toBe('😄 `:smile:` ```\n:smile:\n``` 😄');
  });
  it('does not touch a shortcode inside a word', () => {
    expect(replaceShortcodes('x:smile:', codes)).toBe('x:smile:');
  });
});

describe('suggestShortcodes', () => {
  it('lists the shortest match first', () => {
    expect(suggestShortcodes(codes, 'smi').map((s) => s.name)).toEqual(['smile', 'smiley']);
    expect(suggestShortcodes(codes, 'zzz')).toEqual([]);
  });
});

describe('loadShortcodes', () => {
  it('loads the real data', async () => {
    const map = await loadShortcodes();
    expect(map.get('smile')).toBeTruthy();
    expect(map.get('tada')).toBe('🎉');
    expect(map.get('not_a_real_emoji_name')).toBeUndefined();
  });
});
