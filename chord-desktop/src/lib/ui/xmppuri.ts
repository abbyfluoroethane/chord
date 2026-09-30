// Parses `xmpp:` URIs (RFC 5122) with the query actions of XEP-0147. The functions are
// pure. They never throw and they never touch the network. A link that is malformed, or
// that asks for an action that Chord does not know, gives `{ kind: 'unknown' }`.
//
// Forms that Chord knows:
//   space    xmpp:SERVICE?pubsub;action=subscribe;node=NODE    (XEP-0503: a space is a pubsub node)
//            xmpp:SERVICE?;node=NODE                           (the XEP-0503 form with no action)
//   room     xmpp:ROOM?join[;password=SECRET]
//   chat     xmpp:USER   and   xmpp:USER?message[;body=TEXT]
//   contact  xmpp:USER?roster[;name=NAME]   and   xmpp:USER?subscribe

export type XmppLink =
  | { kind: 'space'; service: string; node: string }
  | { kind: 'room'; jid: string; password: string | null }
  | { kind: 'chat'; jid: string; body: string | null }
  | { kind: 'contact'; jid: string; name: string | null }
  | { kind: 'unknown' };

export type KnownXmppLink = Exclude<XmppLink, { kind: 'unknown' }>;

const UNKNOWN: XmppLink = { kind: 'unknown' };

/** The longest link that we read. A longer one is not a real invite. */
export const MAX_URI_LENGTH = 2048;
const MAX_PART_BYTES = 1023;
const MAX_BODY_LENGTH = 4096;

const encoder = new TextEncoder();
/** Control characters, the line and paragraph separators, and the unassigned marks. */
// eslint-disable-next-line no-control-regex
const BAD_CHARS = /[\u0000-\u001f\u007f-\u009f\u2028\u2029\ufffe\uffff]/;
/** Characters that RFC 7622 does not allow in the local part of a JID. */
const BAD_LOCAL = /[\s"&'/:<>@]/;
const LABEL = /^[\p{L}\p{N}](?:[\p{L}\p{N}-]*[\p{L}\p{N}])?$/u;

function bytes(s: string): number {
  return encoder.encode(s).length;
}

/** A domain name: labels of letters, digits, and hyphens. Lower case. Null if it is bad. */
function domainOf(domain: string): string | null {
  const d = domain.toLowerCase().replace(/\.$/, '');
  if (!d || bytes(d) > MAX_PART_BYTES) return null;
  return d.split('.').every((l) => l.length > 0 && l.length <= 63 && LABEL.test(l)) ? d : null;
}

/** The parts of a JID: local part (or null), domain, and resource (or null). */
function splitJid(s: string): { local: string | null; domain: string; resource: string | null } | null {
  if (!s || BAD_CHARS.test(s)) return null;
  let rest = s;
  let resource: string | null = null;
  // The first slash starts the resource. The local part cannot hold a slash.
  const slash = rest.indexOf('/');
  if (slash >= 0) {
    resource = rest.slice(slash + 1);
    rest = rest.slice(0, slash);
    if (!resource || bytes(resource) > MAX_PART_BYTES) return null;
  }
  let local: string | null = null;
  const at = rest.indexOf('@');
  if (at >= 0) {
    local = rest.slice(0, at);
    rest = rest.slice(at + 1);
    if (!local || bytes(local) > MAX_PART_BYTES || BAD_LOCAL.test(local)) return null;
    local = local.toLowerCase();
  }
  const domain = domainOf(rest);
  return domain ? { local, domain, resource } : null;
}

/** A bare address `user@host`. The resource is dropped. Null if it is not an address. */
export function parseAddress(s: string): string | null {
  const j = splitJid(s.trim());
  return j && j.local ? `${j.local}@${j.domain}` : null;
}

/** A service address: `host`, or `user@host`. The resource is dropped. */
function parseService(s: string): string | null {
  const j = splitJid(s);
  if (!j) return null;
  return j.local ? `${j.local}@${j.domain}` : j.domain;
}

/** Percent-decode one part of the URI. Null if the escapes are bad or the text is not UTF-8. */
function decode(s: string): string | null {
  try {
    const out = decodeURIComponent(s);
    return BAD_CHARS.test(out) ? null : out;
  } catch {
    return null;
  }
}

/** The key-value pairs of a query after the query type. The first value of a key wins. */
function pairs(parts: string[]): Map<string, string> | null {
  const out = new Map<string, string>();
  for (const part of parts) {
    if (!part) continue;
    const eq = part.indexOf('=');
    const key = decode(eq < 0 ? part : part.slice(0, eq));
    const value = decode(eq < 0 ? '' : part.slice(eq + 1));
    if (key === null || value === null) return null;
    if (!out.has(key)) out.set(key, value);
  }
  return out;
}

/** Parse an `xmpp:` URI. Never throws. */
export function parseXmppUri(input: string): XmppLink {
  const uri = input.trim();
  if (!uri || uri.length > MAX_URI_LENGTH || !/^xmpp:/i.test(uri)) return UNKNOWN;
  if (/[\s\u0000-\u001f\u007f]/.test(uri)) return UNKNOWN;
  let rest = uri.slice(5);
  // The fragment has no meaning for Chord.
  const hash = rest.indexOf('#');
  if (hash >= 0) rest = rest.slice(0, hash);
  // `xmpp://authority/path` acts as another account. Chord does not support it.
  if (rest.startsWith('//')) return UNKNOWN;

  const q = rest.indexOf('?');
  const rawPath = q < 0 ? rest : rest.slice(0, q);
  const query = q < 0 ? null : rest.slice(q + 1);
  const path = decode(rawPath);
  if (path === null || !path) return UNKNOWN;

  const [type, ...rawPairs] = (query ?? '').split(';');
  const action = decode(type)?.toLowerCase();
  const params = pairs(rawPairs);
  if (action === undefined || !params) return UNKNOWN;

  if (action === 'pubsub' || (action === '' && params.has('node'))) {
    // A space. `pubsub` needs the `subscribe` action, and the XEP-0503 form has none.
    const given = params.get('action')?.toLowerCase();
    if (action === 'pubsub' && given !== 'subscribe') return UNKNOWN;
    if (action === '' && given !== undefined && given !== 'subscribe') return UNKNOWN;
    const service = parseService(path);
    const node = params.get('node');
    if (!service || !node || !node.trim() || bytes(node) > MAX_PART_BYTES) return UNKNOWN;
    return { kind: 'space', service, node };
  }

  const jid = parseAddress(path);
  if (!jid) return UNKNOWN;
  if (query === null) return { kind: 'chat', jid, body: null };
  switch (action) {
    case 'join': {
      const password = params.get('password') ?? null;
      return { kind: 'room', jid, password: password || null };
    }
    case 'message': {
      const body = params.get('body') ?? null;
      if (body && body.length > MAX_BODY_LENGTH) return UNKNOWN;
      return { kind: 'chat', jid, body: body || null };
    }
    case 'roster':
      return { kind: 'contact', jid, name: params.get('name')?.trim() || null };
    case 'subscribe':
      return { kind: 'contact', jid, name: null };
    default:
      return UNKNOWN;
  }
}

/** True when the text starts like an `xmpp:` URI. It does not check the rest. */
export function looksLikeXmppUri(s: string): boolean {
  return /^xmpp:/i.test(s);
}

/** The key of a link, for caches. Two URIs for the same target have the same key. */
export function xmppKey(link: KnownXmppLink): string {
  switch (link.kind) {
    case 'space':
      return `space:${link.service}/${link.node}`;
    case 'room':
      return `room:${link.jid}`;
    default:
      return `person:${link.jid}`;
  }
}

/** The invite link of a space. The space dialog puts it into a message. */
export function spaceInviteLink(service: string, node: string): string {
  return `xmpp:${service}?pubsub;action=subscribe;node=${encodeURIComponent(node)}`;
}
