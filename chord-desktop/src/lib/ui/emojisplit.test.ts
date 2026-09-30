import { describe, expect, it } from 'vitest';
import { joinStatus, splitEmoji, splitStatus } from './emojisplit';

describe('status emoji', () => {
  it('splits a leading emoji and a space from the text', () => {
    expect(splitStatus('🚀 Static fire today')).toEqual({ emoji: '🚀', text: 'Static fire today' });
    expect(splitStatus('👋🏽 hi')).toEqual({ emoji: '👋🏽', text: 'hi' });
    expect(splitStatus('❤️')).toEqual({ emoji: '❤️', text: '' });
  });

  it('keeps a status with no leading emoji, or an emoji inside a word', () => {
    expect(splitStatus('Prepping the static fire')).toEqual({ emoji: '', text: 'Prepping the static fire' });
    expect(splitStatus('🚀launch')).toEqual({ emoji: '', text: '🚀launch' });
    expect(splitStatus(null)).toEqual({ emoji: '', text: '' });
  });

  it('joins as emoji, space, text', () => {
    expect(joinStatus('🚀', ' Launch ')).toBe('🚀 Launch');
    expect(joinStatus('🚀', '')).toBe('🚀');
    expect(joinStatus('', 'Launch')).toBe('Launch');
    expect(splitStatus(joinStatus('🎉', 'Party'))).toEqual({ emoji: '🎉', text: 'Party' });
  });

  it('finds emoji in text', () => {
    expect(splitEmoji('a 🎉 b').map((p) => p.emoji)).toEqual([false, true, false]);
  });
});
