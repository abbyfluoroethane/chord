import { describe, expect, it } from 'vitest';
import { bumpReaction, DEFAULT_REACTIONS, topReactions } from './reactions';

describe('topReactions', () => {
  it('gives the defaults when nothing was sent', () => {
    expect(topReactions({})).toEqual(DEFAULT_REACTIONS);
  });

  it('puts the most used emoji first and fills with the defaults', () => {
    expect(topReactions({ '🎉': 5, '👀': 2 })).toEqual(['🎉', '👀', '👍', '❤️']);
  });

  it('keeps only four and breaks a tie by the order of the defaults', () => {
    const use = { '🚀': 3, '😂': 3, '🎉': 3, '🔥': 3, '👍': 3 };
    expect(topReactions(use)).toEqual(['👍', '😂', '🚀', '🎉']);
  });

  it('ignores a count of zero', () => {
    expect(topReactions({ '🎉': 0 })).toEqual(DEFAULT_REACTIONS);
  });

  it('counts a new use without changing the old object', () => {
    const use = { '🎉': 1 };
    expect(bumpReaction(use, '🎉')).toEqual({ '🎉': 2 });
    expect(bumpReaction(use, '🔥')).toEqual({ '🎉': 1, '🔥': 1 });
    expect(use).toEqual({ '🎉': 1 });
  });
});
