import { describe, expect, it } from 'vitest';
import { isGroup, memberLine, type ChannelItem } from './types';

const base: ChannelItem = {
  jid: 'crew@conference.foid.space',
  name: 'Crew',
  kind: 'channel',
  unread: 0,
  joined: true,
  mentions: 0,
  muted: false,
  topic: null,
  space: null,
  avatar: null,
  show: null,
  online: false,
};

describe('group chats', () => {
  it('is a room outside a space', () => {
    expect(isGroup(base)).toBe(true);
    expect(isGroup({ ...base, space: 'chat.foid.space/ops' })).toBe(false);
    expect(isGroup({ ...base, kind: 'dm', jid: 'jo@foid.space' })).toBe(false);
    expect(
      isGroup({ ...base, kind: 'dm', pm: { room: base.jid, nick: 'jo' } }),
    ).toBe(false);
  });

  it('counts the members as Discord does', () => {
    expect(memberLine(1)).toBe('1 Member');
    expect(memberLine(4)).toBe('4 Members');
    expect(memberLine(0)).toBe('');
    expect(memberLine(null)).toBe('');
    expect(memberLine(undefined)).toBe('');
  });
});
