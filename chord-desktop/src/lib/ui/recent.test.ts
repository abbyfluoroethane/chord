import { describe, expect, it } from 'vitest';
import { touchRecent } from './recent';

describe('touchRecent', () => {
  it('puts the new key first', () => {
    expect(touchRecent(['a', 'b'], 'c', 5)).toEqual({ recent: ['c', 'a', 'b'], dropped: [] });
  });

  it('moves a known key to the front without a duplicate', () => {
    expect(touchRecent(['a', 'b', 'c'], 'c', 5).recent).toEqual(['c', 'a', 'b']);
  });

  it('drops the oldest keys over the limit', () => {
    expect(touchRecent(['a', 'b', 'c'], 'd', 3)).toEqual({
      recent: ['d', 'a', 'b'],
      dropped: ['c']
    });
  });

  it('does not change the input', () => {
    const input = ['a', 'b'];
    touchRecent(input, 'c', 1);
    expect(input).toEqual(['a', 'b']);
  });

  it('keeps the list at the limit after many opens', () => {
    let r: string[] = [];
    for (let i = 0; i < 50; i++) r = touchRecent(r, `chat${i}`, 8).recent;
    expect(r).toHaveLength(8);
    expect(r[0]).toBe('chat49');
  });
});
