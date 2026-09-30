import { describe, expect, it } from 'vitest';
import type { SearchHit } from '$lib/chord/types';
import { excerpt, matches, searchItems, senderLabel } from './search';
import type { TimelineItem } from './types';

const item = (id: string, body: string, extra: Partial<TimelineItem> = {}): TimelineItem =>
  ({
    id,
    body,
    senderName: 'Rin',
    timestamp: 1,
    outgoing: false,
    retracted: false,
    ...extra
  }) as TimelineItem;

describe('matches', () => {
  it('ignores case and rejects a blank query', () => {
    expect(matches('Lunch at noon', 'LUNCH')).toBe(true);
    expect(matches('Lunch at noon', '  ')).toBe(false);
    expect(matches('Lunch', 'dinner')).toBe(false);
  });
});

describe('searchItems', () => {
  it('returns matches newest first and skips retracted messages', () => {
    const items = [
      item('m:1', 'cat one'),
      item('m:2', 'no'),
      item('m:3', 'cat two', { outgoing: true }),
      item('m:4', 'cat gone', { retracted: true })
    ];
    const hits = searchItems(items, 'rin@example.org', 'cat');
    expect(hits.map((h) => h.id)).toEqual(['m:3', 'm:1']);
    expect(hits[0].direction).toBe('out');
    expect(hits[0].peer).toBe('rin@example.org');
  });
});

describe('excerpt', () => {
  it('splits around the first match and marks the cut', () => {
    const body = `${'a'.repeat(60)} needle ${'b'.repeat(60)}`;
    const e = excerpt(body, 'NEEDLE', 10);
    expect(e.match).toBe('needle');
    expect(e.before.startsWith('…')).toBe(true);
    expect(e.after.endsWith('…')).toBe(true);
  });

  it('keeps a short text whole and flattens line breaks', () => {
    expect(excerpt('one\ntwo', 'two')).toEqual({ before: 'one ', match: 'two', after: '' });
  });

  it('gives the start of a text without a match', () => {
    expect(excerpt('hello', 'zzz').after).toBe('hello');
  });
});

describe('senderLabel', () => {
  const hit = (over: Partial<SearchHit>): SearchHit => ({
    id: 'm:1',
    kind: 'chat',
    direction: 'in',
    peer: 'p@x',
    sender: 'rin@example.org/phone',
    body: '',
    timestamp: 1,
    ...over
  });
  it('names you, the room nick, or the local part', () => {
    expect(senderLabel(hit({ direction: 'out' }))).toBe('you');
    expect(senderLabel(hit({ kind: 'groupchat', sender: 'room@muc.x/Rin' }))).toBe('Rin');
    expect(senderLabel(hit({}))).toBe('rin');
  });
});
