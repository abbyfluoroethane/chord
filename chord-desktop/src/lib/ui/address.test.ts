import { describe, expect, it } from 'vitest';
import { isRoomAnswer, roomByDomain, typedAddress } from './address';

describe('typed addresses', () => {
  it('finds an address, with or without the switcher prefix', () => {
    expect(typedAddress('hikers@conference.foid.space')).toEqual({
      kind: 'address',
      jid: 'hikers@conference.foid.space'
    });
    expect(typedAddress(' #hikers@conference.foid.space ')).toEqual({
      kind: 'address',
      jid: 'hikers@conference.foid.space'
    });
    expect(typedAddress('@Jo@foid.space')).toEqual({ kind: 'address', jid: 'jo@foid.space' });
  });

  it('passes an xmpp: link on as it is', () => {
    expect(typedAddress('xmpp:hikers@conference.foid.space?join')).toEqual({
      kind: 'link',
      uri: 'xmpp:hikers@conference.foid.space?join'
    });
  });

  it('ignores a name or a half address', () => {
    for (const q of ['', 'general', 'jo@', '@jo', 'jo@localhost', 'a b@c.d', 'room@muc.example/nick'])
      expect(typedAddress(q), q).toBeNull();
  });
});

describe('room or person', () => {
  const missing = { code: 'itemNotFound' };
  const refused = { code: 'server', message: 'Not subscribed' };

  it('reads a group chat service from the domain', () => {
    expect(roomByDomain('a@conference.foid.space')).toBe(true);
    expect(roomByDomain('a@muc.example.org')).toBe(true);
    expect(roomByDomain('jo@foid.space')).toBe(false);
    expect(roomByDomain('jo@chat.foid.space')).toBe(false);
  });

  it('decides from the room read', () => {
    expect(isRoomAnswer({ ok: true }, 'x@chat.foid.space')).toBe(true);
    expect(isRoomAnswer({ ok: false, error: missing }, 'new@conference.foid.space')).toBe(true);
    expect(isRoomAnswer({ ok: false, error: refused }, 'jo@chat.foid.space')).toBe(false);
    expect(isRoomAnswer({ ok: false, error: refused }, 'hidden@muc.example.org')).toBe(true);
  });
});
