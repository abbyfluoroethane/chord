// The text of a contacts export. Two formats: vCard 4.0 for other address books, and a
// plain CSV list. No state here, so the rules are easy to test.
import type { ContactItem } from './types';

type Entry = Pick<ContactItem, 'address' | 'name'>;

/** Escape a vCard text value (RFC 6350, 3.4). */
function esc(text: string): string {
  return text.replace(/\\/g, '\\\\').replace(/\r?\n/g, '\\n').replace(/;/g, '\\;').replace(/,/g, '\\,');
}

/** One vCard 4.0 for each contact. The XMPP address goes in IMPP, as `xmpp:` (RFC 6350, 6.4.3). */
export function toVCards(contacts: Entry[]): string {
  return contacts
    .map(
      (c) =>
        ['BEGIN:VCARD', 'VERSION:4.0', `FN:${esc(c.name || c.address)}`, `IMPP:xmpp:${c.address}`, 'END:VCARD'].join(
          '\r\n'
        ) + '\r\n'
    )
    .join('');
}

/** A CSV cell. A leading =, +, - or @ gets a quote mark, so a sheet never runs it as a formula. */
function cell(text: string): string {
  const safe = /^[=+\-@\t\r]/.test(text) ? `'${text}` : text;
  return /[",\r\n]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/** A CSV list with a header row: name and address. */
export function toCsv(contacts: Entry[]): string {
  const rows = contacts.map((c) => `${cell(c.name)},${cell(c.address)}`);
  return ['Name,Address', ...rows].join('\r\n') + '\r\n';
}
