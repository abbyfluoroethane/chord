import { describe, expect, it } from 'vitest';
import type { Pin } from '$lib/chord/types';
import { addPin, pinFromItem, pinLine, pinOf } from './pins';
import type { TimelineItem } from './types';

const item = (over: Partial<TimelineItem> = {}): TimelineItem => ({
  id: 'm:7',
  stanzaId: 'sid-7',
  originId: null,
  sender: 'room@muc/bob',
  senderName: 'bob',
  avatar: null,
  body: 'remember this',
  timestamp: 1000,
  outgoing: false,
  sameSenderAsPrevious: false,
  edited: false,
  retracted: false,
  reactions: [],
  replyTo: null,
  attachment: null,
  status: 'sent',
  mention: false,
  ...over
});

const pin = (over: Partial<Pin> = {}): Pin => ({
  chat: 'room@muc',
  key: 'sid-7',
  itemId: null,
  sender: 'bob',
  body: 'remember this',
  timestamp: 1000,
  pinnedAt: 5,
  ...over
});

describe('pinOf', () => {
  it('matches by the timeline id', () => {
    expect(pinOf([pin({ key: 'x', itemId: 'm:7' })], item())).toBeDefined();
  });
  it('matches a pin of another device by the stanza-id', () => {
    expect(pinOf([pin()], item())).toBeDefined();
    expect(pinOf([pin({ key: 'other' })], item())).toBeUndefined();
  });
  it('matches by the origin-id when there is no stanza-id', () => {
    expect(pinOf([pin({ key: 'o-1' })], item({ stanzaId: null, originId: 'o-1' }))).toBeDefined();
  });
  it('does not match an empty key with a missing id', () => {
    expect(pinOf([pin({ key: '' })], item({ stanzaId: null, originId: null }))).toBeUndefined();
  });
});

describe('pinFromItem and addPin', () => {
  it('prefers the id that the server gave', () => {
    expect(pinFromItem('room@muc', item(), 9).key).toBe('sid-7');
    expect(pinFromItem('room@muc', item({ stanzaId: null }), 9).key).toBe('m:7');
  });
  it('puts the new pin first and keeps one pin for each message', () => {
    const a = pin({ key: 'a' });
    const b = pin({ key: 'b' });
    expect(addPin([a], b).map((p) => p.key)).toEqual(['b', 'a']);
    expect(addPin([a, b], pin({ key: 'a', pinnedAt: 9 })).map((p) => p.key)).toEqual(['a', 'b']);
  });
});

describe('pinLine', () => {
  it('flattens white space and cuts a long text', () => {
    expect(pinLine(pin({ body: 'a\n\n  b' }))).toBe('a b');
    expect(pinLine(pin({ body: 'x'.repeat(200) }), 10)).toHaveLength(10);
    expect(pinLine(pin({ body: '' }))).toBe('Attachment');
  });
});
