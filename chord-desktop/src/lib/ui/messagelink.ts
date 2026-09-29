// Addresses that a menu copies. XEP-0147 (the xmpp URI scheme) has query types for a
// room (`?join`) and for a message that you write (`?message`), but none that names one
// message that already exists. `;id=` in `?message` is the id of a message to send. So
// Chord does not invent a per-message link: the menu copies the address of the channel,
// and the id of the message has its own row.

/** `xmpp:room@service?join` for a room, `xmpp:jid?message` for a person. */
export function channelLink(jid: string, kind: 'channel' | 'dm'): string {
  return kind === 'channel' ? `xmpp:${jid}?join` : `xmpp:${jid}?message`;
}
