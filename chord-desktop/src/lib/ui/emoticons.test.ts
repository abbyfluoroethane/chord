import { describe, expect, it } from 'vitest';
import { replaceEmoticons } from './emoticons';

describe('replaceEmoticons', () => {
  it('replaces an emoticon that stands alone', () => {
    expect(replaceEmoticons('hi :)')).toBe('hi 🙂');
    expect(replaceEmoticons(':D yes ;) <3')).toBe('😄 yes 😉 ❤️');
    expect(replaceEmoticons('great :-)!')).toBe('great 🙂!');
  });
  it('leaves links and words that hold the characters', () => {
    expect(replaceEmoticons('see http://example.org/a:)b')).toBe('see http://example.org/a:)b');
    expect(replaceEmoticons('ratio a:b and XDR')).toBe('ratio a:b and XDR');
  });
  it('leaves code as typed', () => {
    expect(replaceEmoticons('`:)` and :)')).toBe('`:)` and 🙂');
    expect(replaceEmoticons('```\n:)\n```')).toBe('```\n:)\n```');
  });
});
