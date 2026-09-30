// A room or a person address typed into the quick switcher. The app has no other place to
// type one: a room could only be joined from an xmpp: link or an invitation.
import { roomIsMissing } from './roomjoin';
import { looksLikeXmppUri, parseAddress } from './xmppuri';

export type TypedAddress = { kind: 'link'; uri: string } | { kind: 'address'; jid: string };

/**
 * The address or the `xmpp:` link in a query, or null. A leading `#` or `@` (the switcher
 * filters) is dropped. An address needs a local part and a domain with a dot.
 */
export function typedAddress(query: string): TypedAddress | null {
  const s = query.trim().replace(/^[#@]/, '').trim();
  if (!s) return null;
  if (looksLikeXmppUri(s)) return { kind: 'link', uri: s };
  if (!/^[^@\s/]+@[^@\s/]+\.[^@\s/]+$/.test(s)) return null;
  const jid = parseAddress(s);
  return jid ? { kind: 'address', jid } : null;
}

/** The usual names of a group chat service: conference.example.org, muc.example.org. */
const ROOM_SERVICE = /^(conference|muc|rooms?|groupchat|chatrooms?|conf|mucs?)\./i;

/** True when the domain of the address looks like a group chat service. */
export function roomByDomain(jid: string): boolean {
  return ROOM_SERVICE.test(jid.split('@')[1] ?? '');
}

/**
 * Decide from the answer of a room read (disco#info) whether an address is a room. A card
 * means a room. `item-not-found` comes from a group chat service for a room that does not
 * exist yet, and the join then asks before it makes the room. A person answers with another
 * error, unless the domain looks like a group chat service.
 */
export function isRoomAnswer(result: { ok: true } | { ok: false; error: unknown }, jid: string): boolean {
  if (result.ok) return true;
  return roomIsMissing(result.error) || roomByDomain(jid);
}
