// The facts for the Advanced settings page, as plain functions so that tests can reach them.

/**
 * The names of the features that Chord knows. The key is the namespace without its version
 * number. The order is the order of the list on the page.
 */
const NAMES: [string, string][] = [
  ['urn:xmpp:mam', 'Message archive'],
  ['urn:xmpp:http:upload', 'File upload'],
  ['urn:xmpp:push', 'Push notifications'],
  ['urn:xmpp:carbons', 'Message carbons'],
  ['urn:xmpp:sm', 'Stream management'],
  ['urn:xmpp:csi', 'Client state indication'],
  ['urn:xmpp:blocking', 'Blocking'],
  ['urn:xmpp:bookmarks', 'Bookmarks'],
  ['http://jabber.org/protocol/muc', 'Group chat'],
  ['urn:xmpp:mucsub', 'Room subscriptions'],
  ['urn:xmpp:extdisco', 'Call servers'],
  ['urn:xmpp:jingle-message', 'Call invites'],
  ['urn:xmpp:avatar:metadata', 'Avatars'],
  ['urn:xmpp:avatar:data', 'Avatars'],
  ['vcard-temp', 'Profiles'],
  ['urn:ietf:params:xml:ns:vcard-4.0', 'Profiles'],
  ['urn:xmpp:mds:displayed', 'Read state sync'],
  ['urn:xmpp:message-correct', 'Message edits'],
  ['urn:xmpp:message-retract', 'Message deletion'],
  ['urn:xmpp:reactions', 'Reactions'],
  ['urn:xmpp:sid', 'Stanza IDs'],
  ['http://jabber.org/protocol/pubsub', 'Publish and subscribe'],
  ['http://jabber.org/protocol/commands', 'Server commands'],
  ['jabber:iq:register', 'Registration'],
  ['urn:xmpp:ping', 'Ping']
];

const ORDER = new Map(NAMES.map(([ns], i) => [ns, i]));
const LABEL = new Map(NAMES);

/** The namespace without a version number at its end, for example `urn:xmpp:mam:2`. */
function base(ns: string): string {
  return ns.replace(/#.*$/, '').replace(/(?::\d+)+$/, '');
}

/** The name that Chord has for a feature, or null. */
export function featureName(ns: string): string | null {
  return LABEL.get(ns) ?? LABEL.get(base(ns)) ?? null;
}

/** The names of the known features in `list`, each once, in page order. */
export function knownFeatures(list: readonly string[]): string[] {
  const found = new Map<string, number>();
  for (const ns of list) {
    const name = featureName(ns);
    if (!name) continue;
    const at = ORDER.get(ns) ?? ORDER.get(base(ns)) ?? NAMES.length;
    found.set(name, Math.min(at, found.get(name) ?? at));
  }
  return [...found].sort((a, b) => a[1] - b[1]).map(([name]) => name);
}

/** How long the session is up, in words. */
export function uptimeText(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000);
  if (minutes < 1) return 'Under a minute';
  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? '' : 's'}`;
  if (minutes < 60) return plural(minutes, 'minute');
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const m = minutes % 60;
    return m ? `${plural(hours, 'hour')} ${plural(m, 'minute')}` : plural(hours, 'hour');
  }
  const days = Math.floor(hours / 24);
  const h = hours % 24;
  return h ? `${plural(days, 'day')} ${plural(h, 'hour')}` : plural(days, 'day');
}

export interface DebugFacts {
  version: string;
  os: string;
  arch: string;
  server: string;
  status: string;
  features: readonly string[];
}

/** The text that "Copy debug info" puts on the clipboard. It holds no address and no password. */
export function debugText(f: DebugFacts): string {
  return [
    `Chord ${f.version}`,
    `System: ${f.os} ${f.arch}`,
    `Server: ${f.server || 'none'}`,
    `Connection: ${f.status}`,
    `Server features (${f.features.length}): ${f.features.length ? f.features.join(', ') : 'none'}`
  ].join('\n');
}

/** The keys of the settings file that "Reset all settings" removes. */
export const RESET_FILE_KEYS = ['prefs', 'linkPreviews', 'loadFromStrangers'] as const;

/** The localStorage keys that it removes. Hidden chats, notes, and imported themes stay. */
export const RESET_LOCAL_KEYS = [
  'chord.prefs',
  'chord.theme',
  'chord.themePick',
  'chord.skinTone',
  'chord.membersOpen',
  'chord.linkPreviews',
  'chord.loadFromStrangers'
] as const;
