import { describe, expect, it } from 'vitest';
import { buildGroups, decodeEmoji, searchEmoji, withTone, type Raw } from './emojidata';
import gen from './emojidata.gen';
import { compactEmoji, escapeWide, fromHex, toHex, unescapeWide } from './emojibuild';

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

  it('keeps the tags in lower case, and the label as it is', () => {
    expect(groups[0].emoji[1].tags).toBe('smile happy');
    expect(groups[0].emoji[1].label).toBe('grinning face');
    expect(groups[0].emoji[0].tags).toBe('');
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

describe('compact text', () => {
  it('turns an emoji into hex and back', () => {
    expect(toHex('👨‍👩‍👧')).toBe('1f468.200d.1f469.200d.1f467');
    expect(fromHex(toHex('🏳️‍🌈'))).toBe('🏳️‍🌈');
  });

  it('keeps the letters above U+00FF in a label', () => {
    expect(unescapeWide(escapeWide('o’clock 5–0'))).toBe('o’clock 5–0');
    expect(/[^\u0000-\u00ff]/.test(gen)).toBe(false);
    const clock = decodeEmoji(gen).flatMap((g) => g.emoji).find((e) => e.label === 'twelve o’clock');
    expect(clock?.tags).toContain('o’clock');
  });

  it('is the same data as the Emojibase list', async () => {
    // A failure here means that emojidata.gen.ts is old: run scripts/gen-emoji.mjs.
    const { default: data } = await import('emojibase-data/en/data.json');
    expect(gen).toBe(compactEmoji(data as Raw[]));
  });

  it('reads the generated text into nine groups with skins', () => {
    const groups = decodeEmoji(gen);
    expect(groups.length).toBe(9);
    expect(groups.every((g) => g.emoji.length > 0)).toBe(true);
    const wave = groups[1].emoji.find((e) => e.emoji === '👋');
    expect(wave?.skins?.length).toBe(5);
    expect(groups[1].emoji.find((e) => e.emoji === '👋')?.tags).toContain('wave');
  });
});
