import { describe, expect, it } from 'vitest';
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
});
