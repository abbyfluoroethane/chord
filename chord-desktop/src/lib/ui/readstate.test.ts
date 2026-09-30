import { describe, expect, it } from 'vitest';
import { newLineSeen, shouldReadAtBottom } from './readstate';

const seen = { atBottom: true, visible: true, focused: true, unread: 2, mentions: 0 };

describe('reading at the bottom', () => {
  it('reads a chat with unread messages when its bottom is on screen', () => {
    expect(shouldReadAtBottom(seen)).toBe(true);
    expect(shouldReadAtBottom({ ...seen, unread: 0, mentions: 1 })).toBe(true);
  });

  it('does not read a chat that the reader does not see', () => {
    expect(shouldReadAtBottom({ ...seen, atBottom: false })).toBe(false);
    expect(shouldReadAtBottom({ ...seen, visible: false })).toBe(false);
    expect(shouldReadAtBottom({ ...seen, focused: false })).toBe(false);
  });

  it('does nothing when there is nothing to read', () => {
    expect(shouldReadAtBottom({ ...seen, unread: 0, mentions: 0 })).toBe(false);
  });

  it('counts the new line as seen once it is on screen or below the top', () => {
    expect(newLineSeen(null, 100)).toBe(false);
    expect(newLineSeen(40, 100)).toBe(false);
    expect(newLineSeen(100, 100)).toBe(true);
    expect(newLineSeen(500, 100)).toBe(true);
  });
});
