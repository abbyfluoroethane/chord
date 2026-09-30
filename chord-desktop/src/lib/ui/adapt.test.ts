import { describe, expect, it, vi } from 'vitest';

vi.mock('$lib/chord/avatars', () => ({ avatarUrl: (key: string) => `avatar://${key}` }));

import type { ChannelItem, MemberItem, TimelineItem } from '$lib/chord/types';
import {
  connectErrorText,
  isMuted,
  levelToBridge,
  levelToUi,
  mapDiff,
  plainError,
  scopeOf,
  splitPrivate,
  splitRoster,
  toAttachment,
  toChannel,
  toMember,
  toTimelineItem
} from './adapt';
import { serverArg } from './session.svelte';

const ctx = { space: null, muted: false, mentions: 0, presence: null };

function channel(over: Partial<ChannelItem>): ChannelItem {
  return {
    jid: 'rin@foid.space',
    name: 'Rin',
    kind: { type: 'direct' },
    category: null,
    joined: true,
    lastActivity: null,
    unread: 3,
    blocked: false,
    ...over
  };
}

function item(over: Partial<TimelineItem>): TimelineItem {
  return {
    id: 'm:1',
    stanzaId: null,
    originId: null,
    sender: 'rin@foid.space/phone',
    senderName: 'Rin',
    avatar: 'abc',
    body: 'hi',
    timestamp: 1,
    outgoing: false,
    sameSenderAsPrevious: false,
    edited: false,
    retracted: false,
    reactions: [],
    replyTo: null,
    attachment: null,
    status: 'displayed',
    ...over
  };
}

describe('channels', () => {
  it('maps a direct chat', () => {
    const c = toChannel(channel({}), ctx);
    expect(c).toMatchObject({ kind: 'dm', space: null, unread: 3, avatar: 'avatar://rin@foid.space' });
    expect(c.unknownPresence).toBe(true);
  });

  it('maps a room into its circle', () => {
    const c = toChannel(channel({ jid: 'g@chat.foid.space', kind: { type: 'room' } }), {
      ...ctx,
      space: 'chat.foid.space/ops',
      muted: true
    });
    expect(c).toMatchObject({ kind: 'channel', space: 'chat.foid.space/ops', muted: true });
  });

  it('maps a private message channel', () => {
    const c = toChannel(
      channel({
        jid: 'g@chat.foid.space/Bay',
        name: 'Bay',
        kind: { type: 'privateMessage', room: 'g@chat.foid.space', nick: 'Bay' }
      }),
      ctx
    );
    expect(c.kind).toBe('dm');
    expect(c.pm).toEqual({ room: 'g@chat.foid.space', nick: 'Bay' });
    expect(c.name).toBe('Bay in g');
  });
});

describe('timeline', () => {
  it('strips the resource of a chat sender and turns the avatar into a URL', () => {
    const t = toTimelineItem(item({}), { room: false, me: 'me@foid.space' });
    expect(t.sender).toBe('rin@foid.space');
    expect(t.avatar).toBe('avatar://abc');
    expect(t.status).toBe('sent');
  });

  it('uses my address for my messages', () => {
    const t = toTimelineItem(item({ outgoing: true }), { room: false, me: 'me@foid.space' });
    expect(t.sender).toBe('me@foid.space');
  });

  it('resolves the nick of a room message', () => {
    const t = toTimelineItem(item({ sender: 'g@chat.foid.space/Bay' }), {
      room: true,
      me: 'me@foid.space',
      resolve: (n) => (n === 'Bay' ? 'bay@foid.space' : null)
    });
    expect(t.sender).toBe('bay@foid.space');
    const u = toTimelineItem(item({ sender: 'g@chat.foid.space/Kit' }), { room: true, me: 'x@y.z' });
    expect(u.sender).toBe('g@chat.foid.space/Kit');
  });

  it('gives an attachment a name and a type', () => {
    expect(toAttachment('https://up.example/a/burn%2004.png')).toMatchObject({
      name: 'burn 04.png',
      mime: 'image/png'
    });
  });

  it('maps the items of a diff', () => {
    expect(mapDiff({ type: 'insert', index: 1, item: 2 }, (n) => n * 2)).toEqual({
      type: 'insert',
      index: 1,
      item: 4
    });
    expect(mapDiff({ type: 'remove', index: 0 }, (n: number) => n)).toEqual({ type: 'remove', index: 0 });
  });
});

describe('members', () => {
  const base: MemberItem = {
    id: 'Bay',
    name: 'Bay',
    jid: null,
    role: 'moderator',
    affiliation: 'admin',
    show: 'dnd',
    online: true,
    avatar: null
  };

  it('uses room/nick when the room hides the JID', () => {
    expect(toMember(base, 'g@chat.foid.space')).toMatchObject({
      id: 'g@chat.foid.space/Bay',
      role: 'Moderator',
      affiliation: 'admin',
      show: 'dnd'
    });
  });

  it('uses the real JID when there is one', () => {
    expect(toMember({ ...base, jid: 'bay@foid.space', affiliation: 'weird' }, 'g@x').id).toBe('bay@foid.space');
    expect(toMember({ ...base, affiliation: 'weird' }, 'g@x').affiliation).toBe('none');
  });
});

describe('contacts and levels', () => {
  it('splits the roster', () => {
    const c = {
      name: null,
      groups: [],
      approved: false,
      blocked: false,
      online: false,
      show: null,
      status: null
    };
    const r = splitRoster([
      { ...c, jid: 'a@x.y', name: 'A', subscription: 'both', ask: false, online: true, show: 'dnd' },
      { ...c, jid: 'b@x.y', subscription: 'none', ask: true },
      { ...c, jid: 'c@x.y', subscription: 'both', ask: false, blocked: true }
    ]);
    expect(r.contacts.map((x) => x.address)).toEqual(['a@x.y']);
    expect(r.contacts[0].online).toBe(true);
    expect(r.contacts[0].show).toBe('dnd');
    expect(r.outgoing.map((x) => x.name)).toEqual(['b']);
  });

  it('maps the levels', () => {
    expect(levelToUi('none')).toBe('nothing');
    expect(levelToBridge('nothing')).toBe('none');
    expect(isMuted({ level: 'all', muteUntil: 10 }, 5)).toBe(true);
    expect(isMuted({ level: 'all', muteUntil: 10 }, 20)).toBe(false);
    expect(isMuted({ level: 'none', muteUntil: null })).toBe(true);
  });

  it('splits keys', () => {
    expect(scopeOf(null)).toEqual({ type: 'home' });
    expect(scopeOf('chat.foid.space/ops')).toEqual({ type: 'space', service: 'chat.foid.space', node: 'ops' });
    expect(splitPrivate('g@x/Bay')).toEqual({ room: 'g@x', nick: 'Bay' });
    expect(splitPrivate('g@x')).toBeNull();
  });
});

describe('errors and login', () => {
  it('speaks plainly', () => {
    expect(plainError({ code: 'authFailed', message: 'x' })).toBe('Wrong address or password.');
    expect(plainError({ code: 'invalid', message: 'not a JID' })).toBe('not a JID');
    expect(connectErrorText({ type: 'timeout' }, 'foid.space')).toBe('foid.space did not answer in time. Try again.');
  });

  it('builds the server argument', () => {
    expect(serverArg('')).toBe('');
    expect(serverArg('chat.example.com')).toBe('starttls://chat.example.com:5222');
    expect(serverArg('chat.example.com:5223')).toBe('starttls://chat.example.com:5223');
    expect(serverArg('starttls://h:1')).toBe('starttls://h:1');
    expect(serverArg('xmpps://h:1')).toBe('xmpps://h:1');
  });
});
