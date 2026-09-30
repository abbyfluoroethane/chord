import { describe, expect, it } from 'vitest';
import { parentOf, replyChain } from './replychain';
import type { TimelineItem } from './types';

function msg(id: string, replyTo: string | null = null, extra: Partial<TimelineItem> = {}): TimelineItem {
  return {
    id,
    sender: 'a@x',
    senderName: 'A',
    avatar: null,
    body: id,
    timestamp: 0,
    outgoing: false,
    sameSenderAsPrevious: false,
    edited: false,
    retracted: false,
    reactions: [],
    replyTo: replyTo ? { id: replyTo, senderName: 'A', body: '' } : null,
    attachment: null,
    status: 'sent',
    mention: false,
    ...extra
  } as TimelineItem;
}

describe('replyChain', () => {
  const list = [msg('a'), msg('b', 'a'), msg('c', 'b'), msg('d'), msg('e', 'a')];

  it('walks up to the first message and down to the replies', () => {
    expect(replyChain(list, list[1]).map((m) => m.id)).toEqual(['a', 'b', 'c', 'e']);
  });

  it('has only the message when nothing is linked', () => {
    expect(replyChain(list, list[3]).map((m) => m.id)).toEqual(['d']);
  });

  it('matches the id that the server gave', () => {
    const l = [msg('a', null, { stanzaId: 'srv-1' }), msg('b', 'srv-1')];
    expect(parentOf(l, l[1])?.id).toBe('a');
  });

  it('stops when the first message is not loaded', () => {
    const l = [msg('b', 'gone'), msg('c', 'b')];
    expect(replyChain(l, l[1]).map((m) => m.id)).toEqual(['b', 'c']);
  });

  it('survives a loop', () => {
    const l = [msg('a', 'b'), msg('b', 'a')];
    expect(replyChain(l, l[0]).map((m) => m.id)).toEqual(['a', 'b']);
  });
});
