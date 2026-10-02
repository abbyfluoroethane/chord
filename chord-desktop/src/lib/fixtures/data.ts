// Sample data for the UI shell. The bridge replaces all of this later.
import type { LinkPreview } from '$lib/chord/types';
import type {
  ChannelItem,
  ContactItem,
  MemberItem,
  Me,
  PublicCircle,
  Show,
  SpaceItem,
  TimelineItem
} from '$lib/ui/types';
import { spaceKey } from '$lib/ui/types';
import rangeClip from './range-walkthrough.mp4';

export const me: Me = {
  address: 'abby@foid.space',
  name: 'Abby',
  avatar: null,
  show: 'chat',
  status: 'Prepping the static fire'
};

const SVC = 'chat.foid.space';

export const spaces: SpaceItem[] = [
  { service: SVC, node: 'launch-ops', name: 'Launch Ops', avatar: null },
  { service: SVC, node: 'makers', name: 'Bay Area Makers', avatar: null },
  { service: SVC, node: 'rust-cafe', name: 'Rust Cafe', avatar: null },
  { service: SVC, node: 'board-games', name: 'Board Games Night', avatar: null },
  { service: 'rooms.other.example', node: 'garden', name: 'Tiny Garden', avatar: null },
  { service: SVC, node: 'radio', name: 'Radio Club', avatar: null },
  { service: SVC, node: 'books', name: 'Book Club', avatar: null }
];

const K = {
  ops: spaceKey(spaces[0]),
  makers: spaceKey(spaces[1]),
  rust: spaceKey(spaces[2]),
  games: spaceKey(spaces[3]),
  garden: spaceKey(spaces[4]),
  radio: spaceKey(spaces[5]),
  books: spaceKey(spaces[6])
};

function ch(
  space: string,
  node: string,
  name: string,
  extra: Partial<ChannelItem> = {}
): ChannelItem {
  return {
    jid: `${space.split('/')[1]}-${node}@${space.split('/')[0]}`,
    name,
    kind: 'channel',
    unread: 0,
    joined: true,
    mentions: 0,
    muted: false,
    topic: null,
    space,
    avatar: null,
    show: null,
    online: false,
    ...extra
  };
}

function dm(
  name: string,
  address: string,
  show: Show,
  online: boolean,
  extra: Partial<ChannelItem> = {}
): ChannelItem {
  return {
    jid: address,
    name,
    kind: 'dm',
    unread: 0,
    joined: true,
    mentions: 0,
    muted: false,
    topic: null,
    space: null,
    avatar: null,
    show,
    online,
    ...extra
  };
}

export const channels: ChannelItem[] = [
  dm('Rin', 'rin@foid.space', 'chat', true, { unread: 2, mentions: 2 }),
  // A room outside any space: a group chat among the DMs.
  {
    jid: 'pad-crew@conference.foid.space',
    name: 'Pad Crew',
    kind: 'channel',
    unread: 3,
    joined: true,
    mentions: 0,
    muted: false,
    topic: null,
    space: null,
    avatar: null,
    show: null,
    online: false,
    members: 4
  },
  dm('Sam', 'sam@other.example', 'away', true),
  dm('Jo', 'jo@foid.space', 'dnd', true),
  dm('Lee', 'lee@foid.space', null, false),

  ch(K.ops, 'general', 'general', {
    unread: 5,
    mentions: 1,
    topic: 'Static fire is Thursday. Keep it calm and keep it short.'
  }),
  ch(K.ops, 'static-fire', 'static-fire', {
    unread: 2,
    topic: 'Test stand logs and photos',
    category: 'Test stand'
  }),
  ch(K.ops, 'parts', 'parts', { topic: 'Orders, stock, and where things are', category: 'Test stand' }),
  ch(K.ops, 'off-topic', 'off-topic', { muted: true, unread: 9, category: 'Social' }),
  ch(K.ops, 'safety', 'safety', { topic: 'Read this before you touch the stand' }),

  ch(K.makers, 'general', 'general', { unread: 1, topic: 'Doors open at 18:00' }),
  ch(K.makers, 'laser', 'laser-cutter', { mentions: 3, unread: 3 }),
  ch(K.makers, 'wood', 'woodshop'),

  ch(K.rust, 'general', 'general', { topic: 'Coffee first, borrow checker second' }),
  ch(K.rust, 'help', 'help', { unread: 4 }),

  ch(K.games, 'general', 'general', { topic: 'Friday, 19:30' }),
  ch(K.games, 'wishlist', 'wishlist'),

  ch(K.garden, 'general', 'general', { unread: 1 }),
  ch(K.garden, 'seeds', 'seed-swap'),

  ch(K.radio, 'general', 'general'),
  ch(K.books, 'general', 'general', { muted: true }),
  ch(K.books, 'current', 'this-month', { topic: 'We read a short one this time' })
];

export const publicCircles: PublicCircle[] = [
  {
    service: 'rooms.other.example',
    node: 'hikers',
    name: 'Weekend Hikers',
    description: 'Trail reports and rides to the trailhead.',
    members: 214
  },
  {
    service: SVC,
    node: 'synth',
    name: 'Synth Corner',
    description: 'Patches, pedals, and pictures of cables.',
    members: 98
  },
  {
    service: 'chat.example.net',
    node: 'bikes',
    name: 'Bike Repair',
    description: 'Ask before you buy a torque wrench.',
    members: 341
  },
  {
    service: SVC,
    node: 'sourdough',
    name: 'Sourdough',
    description: 'Starters, schedules, and crumb shots.',
    members: 57
  }
];

// --- members ---------------------------------------------------------

function mem(
  id: string,
  name: string,
  affiliation: MemberItem['affiliation'],
  show: Show,
  online: boolean,
  status: string | null = null
): MemberItem {
  // A room gives the moderator role to its owners and admins.
  const role = affiliation === 'owner' || affiliation === 'admin' ? 'Moderator' : null;
  return { id, name, role, affiliation, show, status, online, avatar: null };
}

const cast: MemberItem[] = [
  mem('abby@foid.space', 'Abby', 'owner', 'chat', true, 'Building the space'),
  mem('rin@foid.space', 'Rin', 'admin', 'chat', true, 'Range safety today'),
  mem('jo@foid.space', 'Jo', 'admin', 'dnd', true, 'Heads down'),
  mem('sam@other.example', 'Sam', 'member', 'away', true, 'At lunch'),
  mem('bay@foid.space', 'Bay', 'member', 'chat', true),
  mem('kit@foid.space', 'Kit', 'member', null, false),
  mem('mo@foid.space', 'Mo', 'member', null, false),
  mem('ivy@foid.space', 'Ivy', 'member', 'xa', false),
  mem('lee@foid.space', 'Lee', 'member', null, false)
];

export const members: Record<string, MemberItem[]> = {
  ...Object.fromEntries(
    Object.values(K).map((k, i) => [k, cast.filter((_, j) => j < 3 || (j + i) % 2 === 0 || j === 3)])
  ),
  // The group chat on the home list keeps its members under the home key.
  home: cast.filter((c) =>
    ['abby@foid.space', 'rin@foid.space', 'bay@foid.space', 'kit@foid.space'].includes(c.id)
  )
};

// --- timelines -------------------------------------------------------

const DAY = 86_400_000;
const startOfToday = new Date().setHours(0, 0, 0, 0);
/** Milliseconds for a clock time some days ago. */
function at(daysAgo: number, h: number, m: number): number {
  return startOfToday - daysAgo * DAY + (h * 60 + m) * 60_000;
}

type Draft = Partial<TimelineItem> & { id: string; who: string; body: string; ts: number };

const names: Record<string, string> = Object.fromEntries(cast.map((c) => [c.id, c.name]));
const addr = (short: string) =>
  short === 'sam' ? 'sam@other.example' : `${short}@foid.space`;

function build(drafts: Draft[]): TimelineItem[] {
  const out: TimelineItem[] = [];
  for (const draft of drafts) {
    const { who, ts, ...d } = draft;
    const sender = addr(who);
    const prev = out[out.length - 1];
    out.push({
      sender,
      senderName: names[sender] ?? who,
      avatar: null,
      timestamp: ts,
      outgoing: sender === me.address,
      sameSenderAsPrevious:
        !!prev && prev.sender === sender && !d.replyTo && ts - prev.timestamp < 7 * 60_000,
      edited: false,
      retracted: false,
      reactions: [],
      replyTo: null,
      attachment: null,
      status: 'sent',
      mention: false,
      ...d
    } as TimelineItem);
  }
  return out;
}

function pic(w: number, h: number, a: string, b: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="${w}" height="${h}" fill="url(#g)"/><circle cx="${w * 0.7}" cy="${h * 0.35}" r="${h * 0.16}" fill="#f6f4ef" opacity=".85"/><path d="M0 ${h} L${w * 0.35} ${h * 0.55} L${w * 0.55} ${h * 0.78} L${w * 0.8} ${h * 0.5} L${w} ${h * 0.8} V${h} Z" fill="#111316" opacity=".55"/></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

/** A short sine tone as a WAV data URI: sample audio for the preview, no network. */
function tone(seconds: number, hz: number): string {
  const rate = 8000;
  const n = Math.floor(seconds * rate);
  const bytes = new Uint8Array(44 + n);
  const view = new DataView(bytes.buffer);
  const text = (at: number, s: string) => [...s].forEach((c, i) => (bytes[at + i] = c.charCodeAt(0)));
  text(0, 'RIFF');
  view.setUint32(4, 36 + n, true);
  text(8, 'WAVEfmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, rate, true);
  view.setUint32(28, rate, true);
  view.setUint16(32, 1, true);
  view.setUint16(34, 8, true); // 8-bit
  text(36, 'data');
  view.setUint32(40, n, true);
  for (let i = 0; i < n; i++) {
    const fade = Math.min(1, i / 400, (n - i) / 400);
    bytes[44 + i] = 128 + Math.round(60 * fade * Math.sin((2 * Math.PI * hz * i) / rate));
  }
  let bin = '';
  bytes.forEach((b) => (bin += String.fromCharCode(b)));
  return `data:audio/wav;base64,${btoa(bin)}`;
}

const opsGeneral = build([
  { id: 'g1', who: 'rin', ts: at(1, 16, 40), body: 'Test stand is bolted down. Torque marks are on all eight bolts.' },
  {
    id: 'g2',
    who: 'bay',
    ts: at(1, 16, 52),
    body: 'Nice. I will bring the load cell calibration sheet tomorrow.',
    reactions: [{ emoji: '👍', count: 3, mine: true }]
  },
  { id: 'g3', who: 'bay', ts: at(1, 16, 53), body: 'Also `cal_2026_09.csv` is in the shared folder.' },
  { id: 'g4', who: 'jo', ts: at(1, 21, 10), body: 'Reminder: nobody runs the igniter alone.' },
  {
    id: 'g5',
    who: 'abby',
    ts: at(0, 9, 5),
    body: 'Morning. Static fire moved to 14:00 tomorrow btw.',
    reactions: [
      { emoji: '🔥', count: 4, mine: false },
      { emoji: '👀', count: 2, mine: true }
    ]
  },
  {
    id: 'g6',
    who: 'rin',
    ts: at(0, 9, 20),
    body: 'Copy that. I will move the range booking.',
    replyTo: { id: 'g5', senderName: 'Abby', body: 'Morning. Static fire moved to 14:00 tomorrow btw.' }
  },
  {
    id: 'g6b',
    who: 'abby',
    ts: at(0, 9, 24),
    body: 'Thanks. Tell the range office about the noise too.',
    replyTo: { id: 'g6', senderName: 'Rin', body: 'Copy that. I will move the range booking.' }
  },
  {
    id: 'g7',
    who: 'sam',
    ts: at(0, 10, 2),
    body: 'Photos from the last burn, for the log. Sorry they came out orange.',
    attachment: {
      url: pic(640, 400, '#c47b0c', '#3a2c16'),
      name: 'burn-04.jpg',
      mime: 'image/jpeg',
      size: 2_483_200,
      width: 640,
      height: 400
    }
  },
  {
    id: 'g7b',
    who: 'sam',
    ts: at(0, 10, 2),
    body: '',
    attachment: {
      url: pic(400, 560, '#0a655c', '#111316'),
      name: 'burn-05.jpg',
      mime: 'image/jpeg',
      size: 1_904_640,
      width: 400,
      height: 560
    }
  },
  {
    id: 'g7c',
    who: 'rin',
    ts: at(0, 10, 2),
    body: 'Igniter test, with sound.',
    attachment: {
      url: tone(2, 440),
      name: 'igniter-test.wav',
      mime: 'audio/wav',
      size: 16_044,
      width: null,
      height: null
    }
  },
  {
    id: 'g7d',
    who: 'jo',
    ts: at(0, 10, 40),
    body: 'Walkthrough of the new stand.',
    attachment: {
      url: rangeClip,
      name: 'range-walkthrough.mp4',
      mime: 'video/mp4',
      size: 26_631,
      width: 640,
      height: 360
    }
  },
  { id: 'g8', who: 'sam', ts: at(0, 10, 3), body: 'Full run sheet is here if you want it.', attachment: {
      url: 'file:///run-sheet.pdf',
      name: 'run-sheet-v3.pdf',
      mime: 'application/pdf',
      size: 184_320,
      width: null,
      height: null
    } },
  // A person who is not a contact. With "Load files from people who are not contacts" off,
  // the photo shows as a plain file link and the link preview does not show.
  {
    id: 'g8s',
    who: 'noor',
    ts: at(0, 10, 4),
    body: 'Hi all, I found your launch log here: https://example.org/launch-log',
    attachment: {
      url: pic(480, 320, '#5b4bb7', '#16131f'),
      name: 'noor-pad.jpg',
      mime: 'image/jpeg',
      size: 96_000,
      width: 480,
      height: 320
    }
  },
  {
    id: 'g9',
    who: 'jo',
    ts: at(0, 11, 30),
    body: 'This message was a mistake.',
    retracted: true
  },
  {
    id: 'g10',
    who: 'kit',
    ts: at(0, 12, 14),
    body: 'Docs say 12 bar max. https://example.org/stand-manual is the link.',
    edited: true
  },
  {
    id: 'g10b',
    who: 'sam',
    ts: at(0, 12, 20),
    body: 'Range weather for Thursday: https://weather.example.net/range and the photo https://cdn.example.net/burn.jpg'
  },
  { id: 'g10c', who: 'kit', ts: at(0, 12, 21), body: '/me checks the **gauge** twice' },
  {
    id: 'g10d',
    who: 'abby',
    ts: at(0, 12, 22),
    body: 'Sending the range sheet now.',
    status: 'failed'
  },
  {
    id: 'g11',
    who: 'rin',
    ts: at(0, 12, 40),
    body: '@abby can you sign off the checklist before 13:00?',
    mention: true
  },
  { id: 'g12', who: 'bay', ts: at(0, 12, 55), body: 'Checklist is in the pinned note.' },
  { id: 'g13', who: 'bay', ts: at(0, 12, 56), body: 'Second page has the valve order.' },
  {
    id: 'g14',
    who: 'jo',
    ts: at(0, 13, 12),
    body: 'Heads up: the north door sticks. Push, do not pull.',
    reactions: [{ emoji: '😂', count: 1, mine: false }]
  },
  {
    id: 'g15',
    who: 'rin',
    ts: at(0, 13, 20),
    body:
      '# Run plan\n' +
      'Static fire is <t:' + Math.floor((at(0, 14, 0) + DAY) / 1000) + ':F>, which is <t:' + Math.floor((at(0, 14, 0) + DAY) / 1000) + ':R>.\n' +
      '> Keep the **range** clear.\n' +
      '- Igniter check\n' +
      '  - Wire the *arming key*\n' +
      '- Valve order\n' +
      '-# The plan is a draft. Ask Jo for changes. :rocket:'
  },
  {
    id: 'g16',
    who: 'bay',
    ts: at(0, 13, 24),
    body:
      'The log script:\n```python\nfor row in rows:\n    if row.pressure > 12:\n        print("high", row.id)\n```\n' +
      'The fix is ||a loose fitting||. Full notes are in [the wiki](https://example.org/wiki/stand).'
  },
  {
    id: 'g16b',
    who: 'sam',
    ts: at(0, 13, 26),
    body: 'I made a space for people who love synths. Join here: xmpp:chat.foid.space?pubsub;action=subscribe;node=synth'
  },
  {
    id: 'g16c',
    who: 'kit',
    ts: at(0, 13, 28),
    body: 'The hangar crew talks in this room: xmpp:hangar@chat.foid.space?join'
  },
  { id: 'g17', who: 'jo', ts: at(0, 13, 30), body: ':tada: :fire:' }
]);

function small(who: string[], base: number): TimelineItem[] {
  const lines = [
    'Anyone here yet?',
    'Yes. Kettle is on.',
    'I will be ten minutes late.',
    'No problem, we start with the boring part.',
    'Bring the cable, the long one.'
  ];
  return build(
    who.slice(0, lines.length).map((w, i) => ({
      id: `${w}-${base}-${i}`,
      who: w,
      ts: at(0, 8, base + i * 6),
      body: lines[i]
    }))
  );
}

/** Sample link previews for the browser preview, keyed by URL. There is no network. */
export const linkPreviews: Record<string, LinkPreview> = {
  'https://example.org/stand-manual': {
    url: 'https://example.org/stand-manual',
    siteName: 'Example Docs',
    title: 'Test stand manual: pressure limits and valve order',
    description:
      'The maximum working pressure is 12 bar. Follow the valve order on page two before every static fire, and log each run in the range book.',
    image: pic(160, 160, '#0a655c', '#111316'),
    imageWidth: 160,
    imageHeight: 160
  },
  'https://weather.example.net/range': {
    url: 'https://weather.example.net/range',
    siteName: 'Range Weather',
    title: 'Thursday at the range: clear, light wind',
    description: null,
    image: pic(1200, 630, '#4cc3b5', '#21252b'),
    imageWidth: 1200,
    imageHeight: 630
  },
  'https://cdn.example.net/burn.jpg': {
    url: 'https://cdn.example.net/burn.jpg',
    siteName: null,
    title: null,
    description: null,
    image: pic(640, 400, '#c47b0c', '#3a2c16'),
    imageWidth: null,
    imageHeight: null
  }
};

/** Rooms that an xmpp: link can name and that the user has not joined. Keyed by address. */
export const xmppRooms: Record<string, { name: string; subject: string | null; occupants: number | null }> = {
  'hangar@chat.foid.space': {
    name: 'hangar',
    subject: 'Assembly bay schedule and tool checkout',
    occupants: 12
  }
};

export const timelines: Record<string, TimelineItem[]> = {
  'launch-ops-general@chat.foid.space': opsGeneral,
  'launch-ops-static-fire@chat.foid.space': small(['bay', 'rin', 'bay', 'jo'], 10),
  'makers-laser@chat.foid.space': small(['kit', 'abby', 'kit'], 20),
  'rust-cafe-help@chat.foid.space': small(['sam', 'rin', 'sam', 'bay', 'sam'], 30),
  'rin@foid.space': build([
    { id: 'd1', who: 'abby', ts: at(0, 8, 30), body: 'Can you cover the range on Thursday?' },
    { id: 'd2', who: 'rin', ts: at(0, 12, 41), body: 'Yes. One more thing about the checklist.', mention: true },
    { id: 'd3', who: 'rin', ts: at(0, 12, 42), body: 'The valve order on page two is wrong.', mention: true }
  ]),
  'pad-crew@conference.foid.space': small(['bay', 'abby', 'rin', 'kit', 'bay'], 50),
  'sam@other.example': small(['sam', 'abby'], 40),
  'jo@foid.space': [],
  'lee@foid.space': []
};

/** Read state: the first unread message per channel, for the "new" divider. */
export const firstUnread: Record<string, string> = {
  'launch-ops-general@chat.foid.space': 'g10',
  'rin@foid.space': 'd2'
};

export const typing: Record<string, string[]> = {
  'launch-ops-general@chat.foid.space': ['Bay']
};

// --- contacts --------------------------------------------------------

function contact(
  address: string,
  name: string,
  show: Show,
  online: boolean,
  status: string | null = null,
  daysAgo: number | null = null
): ContactItem {
  return {
    address,
    name,
    avatar: null,
    show,
    online,
    status,
    since: daysAgo === null ? null : at(daysAgo, 12, 0)
  };
}

export const contacts: ContactItem[] = [
  contact('rin@foid.space', 'Rin', 'chat', true, 'On the range until 17:00', 210),
  contact('sam@other.example', 'Sam', 'away', true, 'Back in ten minutes', 95),
  contact('jo@foid.space', 'Jo', 'dnd', true, 'Deep work. Text if it burns.', 320),
  contact('lee@foid.space', 'Lee', null, false, null, 40),
  contact('bay@foid.space', 'Bay', 'chat', true, null, 150),
  contact('kit@foid.space', 'Kit', null, false, 'Gone climbing', 60),
  contact('ivy@foid.space', 'Ivy', 'xa', false, null, 12)
];

export const incomingRequests: ContactItem[] = [
  contact('noor@chord.example', 'Noor', null, false),
  contact('tam@rooms.other.example', 'Tam', null, false)
];

export const outgoingRequests: ContactItem[] = [contact('wren@chord.example', 'Wren', null, false)];

export const blockedContacts: ContactItem[] = [
  contact('spam@junk.example', 'Spam Bot', null, false)
];

/** Contacts you and this address both have. Sample data only. */
export const sharedContacts: Record<string, string[]> = {
  'rin@foid.space': ['bay@foid.space', 'jo@foid.space'],
  'sam@other.example': ['rin@foid.space'],
  'jo@foid.space': ['rin@foid.space', 'kit@foid.space'],
  'bay@foid.space': ['rin@foid.space', 'ivy@foid.space', 'kit@foid.space']
};

/** Sample GIF results for the picker in the preview. Real results come from KLIPY. */
export const gifs: import('$lib/chord').Gif[] = [
  ['wave', 'Wave', 320, 240, '#c47b0c', '#3a2c16'],
  ['launch', 'Rocket launch', 240, 320, '#0a655c', '#111316'],
  ['thumbs-up', 'Thumbs up', 320, 180, '#f2a93b', '#21252b'],
  ['party', 'Party', 280, 280, '#4cc3b5', '#181b20'],
  ['coffee', 'Coffee time', 320, 200, '#7a4a00', '#111316'],
  ['fire', 'This is fine', 300, 220, '#f0676b', '#3a2c16'],
  ['cat', 'Cat typing', 240, 240, '#5bc46e', '#181b20'],
  ['nod', 'Nod', 320, 260, '#9aa0a8', '#21252b']
].map(([slug, title, w, h, a, b]) => {
  const file = { url: pic(w as number, h as number, a as string, b as string), width: w as number, height: h as number };
  return { slug: slug as string, title: title as string, preview: file, full: file };
});
