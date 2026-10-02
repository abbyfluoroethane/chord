import { describe, expect, it } from 'vitest';
import { buildGroups, searchEmoji, withTone, type Raw } from './emojidata';

const raw: Raw[] = [
  { emoji: '😀', label: 'grinning face', tags: ['smile', 'happy'], group: 0, order: 2, version: 1 },
  { emoji: '😃', label: 'grinning face with big eyes', tags: ['mouth'], group: 0, order: 3, version: 1 },
  { emoji: '🙂', label: 'slightly smiling face', group: 0, order: 1, version: 1 },
  {
    emoji: '👋',
    label: 'waving hand',
    group: 1,
    order: 10,
    version: 1,
    skins: [5, 3, 1, 2, 4].map((tone) => ({ emoji: `👋${tone}`, tone }))
  },
  { emoji: '🏻', label: 'light skin tone', group: 2, order: 20, version: 1 },
  { emoji: '🫨', label: 'too new', group: 0, order: 4, version: 17 },
  { emoji: '🐱', label: 'cat face', group: 3, order: 30, version: 1 }
];

describe('buildGroups', () => {
  const groups = buildGroups(raw);

  it('keeps the picker order and sorts each group by the Emojibase order', () => {
    expect(groups.map((g) => g.id)).toEqual([
      'smileys',
      'people',
      'nature',
      'food',
      'travel',
      'activities',
      'objects',
      'symbols',
      'flags'
    ]);
    expect(groups[0].emoji.map((e) => e.emoji)).toEqual(['🙂', '😀', '😃']);
  });

  it('drops the skin tone swatches and the emoji that are too new', () => {
    const all = groups.flatMap((g) => g.emoji.map((e) => e.emoji));
    expect(all).not.toContain('🏻');
    expect(all).not.toContain('🫨');
  });

  it('puts the skin tones in order, and only when there are five', () => {
    const hand = groups[1].emoji[0];
    expect(hand.skins).toEqual(['👋1', '👋2', '👋3', '👋4', '👋5']);
    expect(groups[0].emoji[0].skins).toBeNull();
  });

  it('writes the search words in lower case', () => {
    expect(groups[0].emoji[1].words).toBe('grinning face smile happy');
  });
});

describe('searchEmoji', () => {
  const groups = buildGroups(raw);

  it('returns nothing for an empty search', () => {
    expect(searchEmoji(groups, '  ')).toEqual([]);
  });

  it('puts a word that starts with the text before a word that contains it', () => {
    expect(searchEmoji(groups, 'mile').map((e) => e.emoji)).toEqual(['😀']);
    expect(searchEmoji(groups, 'smil').map((e) => e.emoji)).toEqual(['🙂', '😀']);
    const mixed = buildGroups([
      { emoji: 'A', label: 'bat', group: 0, order: 1, version: 1 },
      { emoji: 'B', label: 'at once', group: 0, order: 2, version: 1 }
    ]);
    expect(searchEmoji(mixed, 'at').map((e) => e.emoji)).toEqual(['B', 'A']);
    expect(searchEmoji(groups, 'face').map((e) => e.emoji)).toEqual(['🙂', '😀', '😃', '🐱']);
  });

  it('is not case sensitive and stops at the maximum', () => {
    expect(searchEmoji(groups, 'GRINNING').length).toBe(2);
    expect(searchEmoji(groups, 'face', 2).length).toBe(2);
  });
});

describe('withTone', () => {
  const hand = buildGroups(raw)[1].emoji[0];
  it('picks the skin tone, or the default', () => {
    expect(withTone(hand, 0)).toBe('👋');
    expect(withTone(hand, 3)).toBe('👋3');
    expect(withTone(buildGroups(raw)[0].emoji[0], 3)).toBe('🙂');
  });
});
