import { describe, expect, it } from 'vitest';
import { toCsv, toVCards } from './contactsexport';

const list = [
  { address: 'bob@example.org', name: 'Bob, the "Builder"' },
  { address: 'eve@example.org', name: '=SUM(A1)' }
];

describe('toVCards', () => {
  it('writes one card each, with the address in IMPP', () => {
    const out = toVCards(list);
    expect(out.match(/BEGIN:VCARD/g)).toHaveLength(2);
    expect(out).toContain('IMPP:xmpp:bob@example.org\r\n');
    expect(out).toContain('FN:Bob\\, the "Builder"\r\n');
    expect(out.endsWith('END:VCARD\r\n')).toBe(true);
  });
  it('falls back to the address when a contact has no name', () => {
    expect(toVCards([{ address: 'a@b.c', name: '' }])).toContain('FN:a@b.c\r\n');
  });
  it('is empty for no contacts', () => {
    expect(toVCards([])).toBe('');
  });
});

describe('toCsv', () => {
  it('quotes cells and starts with a header', () => {
    const lines = toCsv(list).split('\r\n');
    expect(lines[0]).toBe('Name,Address');
    expect(lines[1]).toBe('"Bob, the ""Builder""",bob@example.org');
  });
  it('stops a formula in a name', () => {
    expect(toCsv(list).split('\r\n')[2]).toBe("'=SUM(A1),eve@example.org");
  });
});
