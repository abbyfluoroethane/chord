import { describe, expect, it } from 'vitest';
import { app } from './app.svelte';
import { contactsStore as c } from './contacts.svelte';

describe('contacts store', () => {
  it('rejects an address that is not valid', () => {
    const r = c.add('sam');
    expect(r.ok).toBe(false);
  });

  it('rejects an address that is already a contact', () => {
    const r = c.add('rin@foid.space');
    expect(r).toEqual({ ok: false, error: 'rin@foid.space is already a contact.' });
  });

  it('sends a request and then refuses a second one', () => {
    expect(c.add('new@chord.example').ok).toBe(true);
    expect(c.outgoing.some((x) => x.address === 'new@chord.example')).toBe(true);
    expect(c.add('new@chord.example').ok).toBe(false);
  });

  it('moves an incoming request to contacts on accept', () => {
    const before = c.contacts.length;
    c.accept('noor@chord.example');
    expect(c.contacts.length).toBe(before + 1);
    expect(c.incoming.some((x) => x.address === 'noor@chord.example')).toBe(false);
  });

  it('blocks and unblocks', () => {
    c.block('jo@foid.space');
    expect(c.isBlocked('jo@foid.space')).toBe(true);
    expect(c.isContact('jo@foid.space')).toBe(false);
    c.unblock('jo@foid.space');
    expect(c.isBlocked('jo@foid.space')).toBe(false);
  });

  it('unblocks everyone at once', () => {
    c.block('jo@foid.space');
    c.block('rin@foid.space');
    c.unblockAll();
    expect(c.blocked).toEqual([]);
  });

  it('accepts a request with add back', () => {
    c.incoming.push({
      address: 'kit@chord.example',
      name: 'kit',
      avatar: null,
      show: null,
      online: false,
      status: null,
      since: null
    });
    c.accept('kit@chord.example', true);
    expect(c.isContact('kit@chord.example')).toBe(true);
  });
});

describe('contacts store lookups', () => {
  it('finds a person in the members of any space', () => {
    app.members['other-space'] = [
      { id: 'zed@chord.example', name: 'Zed', avatar: null, show: 'away', online: true } as never
    ];
    const p = c.person('zed@chord.example');
    expect(p.name).toBe('Zed');
    expect(p.online).toBe(true);
    delete app.members['other-space'];
  });

  it('empties the lists on reset', () => {
    c.reset();
    expect(c.contacts).toEqual([]);
    expect(c.incoming).toEqual([]);
    expect(c.outgoing).toEqual([]);
    expect(c.blocked).toEqual([]);
  });
});

describe('space badges', () => {
  it('sum the unread of each space in one pass and skip muted channels', () => {
    app.channels = [
      { jid: 'a@x', space: 's1', unread: 2, mentions: 1, muted: false },
      { jid: 'b@x', space: 's1', unread: 5, mentions: 0, muted: true },
      { jid: 'c@x', space: null, unread: 3, mentions: 0, muted: false }
    ] as never;
    expect(app.spaceBadge('s1')).toEqual({ unread: 2, mentions: 1 });
    expect(app.spaceBadge('home')).toEqual({ unread: 3, mentions: 0 });
    expect(app.spaceBadge('none')).toEqual({ unread: 0, mentions: 0 });
  });
});
