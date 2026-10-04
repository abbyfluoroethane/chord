// Sample data for the UI shell. The bridge replaces all of this later.
// The story (people, spaces, messages) is in docs/brand/showcase/README.md. The Android
// showcase uses the same story, so keep the two in step. The art in ./showcase is a copy of
// the PNGs in docs/brand/showcase.
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
import adaPng from './showcase/ada.png';
import julesPng from './showcase/jules.png';
import kenjiPng from './showcase/kenji.png';
import luisPng from './showcase/luis.png';
import mayaPng from './showcase/maya.png';
import noorPng from './showcase/noor.png';
import priyaPng from './showcase/priya.png';
import theoPng from './showcase/theo.png';
import lanternPng from './showcase/lantern-works.png';
import darkroomPng from './showcase/darkroom.png';
import cragPng from './showcase/crag.png';
import readersPng from './showcase/slow-readers.png';
import lighthousePng from './showcase/lighthouse-fog.png';
import harborPng from './showcase/harbor-sunset.png';
import hillsPng from './showcase/hills.png';
import patchNotesPng from './showcase/patch-notes.png';

const SVC = 'chat.foid.space';

// --- people ----------------------------------------------------------

interface Person {
  address: string;
  name: string;
  avatar: string | null;
  show: Show;
  online: boolean;
  status: string | null;
}

const person = (
  address: string,
  name: string,
  avatar: string | null,
  show: Show,
  online: boolean,
  status: string | null = null
): Person => ({ address, name, avatar, show, online, status });

/** Everyone in the story, keyed by first name in lower case. */
const P: Record<string, Person> = {
  maya: person('maya@chat.foid.space', 'Maya Okafor', mayaPng, 'chat', true, 'Playtest week'),
  priya: person('priya@chat.foid.space', 'Priya Raman', priyaPng, 'chat', true),
  theo: person('theo@chat.foid.space', 'Theo Lindqvist', theoPng, 'chat', true, 'Profiling saves'),
  jules: person('jules@chat.foid.space', 'Jules Moreau', julesPng, 'away', true),
  kenji: person('kenji@chat.foid.space', 'Kenji Ito', kenjiPng, 'chat', true),
  ada: person('ada@chat.foid.space', 'Ada Novak', adaPng, 'dnd', true, 'Writing'),
  noor: person('noor@xmpp.example.net', 'Noor Haddad', noorPng, 'chat', true),
  luis: person('luis@chat.foid.space', 'Luis Ortega', luisPng, null, false),
  sam: person('sam@chat.foid.space', 'Sam Delgado', null, 'away', true),
  wren: person('wren@chat.foid.space', 'Wren Callahan', null, null, false),
  // Not a contact: she sent a contact request. The Sandbox uses her for a stranger's message.
  mika: person('mika@xmpp.example.net', 'Mika Sato', null, null, false)
};

export const me: Me = {
  address: P.maya.address,
  name: P.maya.name,
  avatar: P.maya.avatar,
  show: P.maya.show,
  status: P.maya.status
};

// --- spaces and channels ---------------------------------------------

export const spaces: SpaceItem[] = [
  { service: SVC, node: 'lantern-works', name: 'Lantern Works', avatar: lanternPng },
  { service: SVC, node: 'darkroom', name: 'Darkroom', avatar: darkroomPng },
  { service: SVC, node: 'crag-club', name: 'Crag Club', avatar: cragPng },
  { service: SVC, node: 'slow-readers', name: 'Slow Readers', avatar: readersPng },
  { service: SVC, node: 'sandbox', name: 'Sandbox', avatar: null }
];

const K = {
  lantern: spaceKey(spaces[0]),
  darkroom: spaceKey(spaces[1]),
  crag: spaceKey(spaces[2]),
  readers: spaceKey(spaces[3]),
  sandbox: spaceKey(spaces[4])
};

/** The address of a channel in a space. */
const room = (space: string, node: string) => `${space.split('/')[1]}-${node}@${space.split('/')[0]}`;

function ch(
  space: string,
  node: string,
  name: string,
  extra: Partial<ChannelItem> = {}
): ChannelItem {
  return {
    jid: room(space, node),
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

function dm(p: Person, extra: Partial<ChannelItem> = {}): ChannelItem {
  return {
    jid: p.address,
    name: p.name,
    kind: 'dm',
    unread: 0,
    joined: true,
    mentions: 0,
    muted: false,
    topic: null,
    space: null,
    avatar: p.avatar,
    show: p.show,
    online: p.online,
    ...extra
  };
}

const LANTERN_CORE = 'lantern-core@conference.chat.foid.space';

export const channels: ChannelItem[] = [
  dm(P.theo, { unread: 1 }),
  dm(P.priya),
  // A room outside any space: a group chat among the DMs.
  {
    jid: LANTERN_CORE,
    name: 'Lantern core',
    kind: 'channel',
    unread: 2,
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
  dm(P.noor),
  dm(P.ada),
  dm(P.jules),

  ch(K.lantern, 'announcements', 'announcements', { unread: 1, topic: 'Builds and dates. Read only.' }),
  ch(K.lantern, 'general', 'general', { unread: 4 }),
  ch(K.lantern, 'playtest', 'playtest', {
    unread: 3,
    topic: 'Build 0.14.2 is live. Bugs go in #bugs.',
    category: 'Playtest'
  }),
  ch(K.lantern, 'bugs', 'bugs', { unread: 3, category: 'Playtest' }),
  ch(K.lantern, 'ideas', 'ideas', { category: 'Playtest' }),
  ch(K.lantern, 'art', 'art', { unread: 1, category: 'Studio' }),
  ch(K.lantern, 'audio', 'audio', { category: 'Studio' }),
  ch(K.lantern, 'off-topic', 'off-topic', { muted: true, unread: 12, category: 'Studio' }),

  ch(K.darkroom, 'general', 'general'),
  ch(K.darkroom, 'show-and-tell', 'show-and-tell', { unread: 2 }),
  ch(K.darkroom, 'developing', 'developing'),
  ch(K.darkroom, 'gear-swap', 'gear-swap'),

  ch(K.crag, 'general', 'general'),
  ch(K.crag, 'sessions', 'sessions', { unread: 1, topic: 'Tue and Thu, 18:30' }),

  ch(K.readers, 'general', 'general'),
  ch(K.readers, 'this-month', 'this-month', {
    topic: 'The Left Hand of Darkness, ch. 1 to 8'
  }),

  ch(K.sandbox, 'general', 'general', {
    topic: 'Test messages for the preview: every kind of message in one place'
  })
];

export const publicCircles: PublicCircle[] = [
  {
    service: SVC,
    node: 'night-trains',
    name: 'Night Trains',
    description: 'Routes, sleeper cars, and tips for long trips by rail.',
    members: 412
  },
  {
    service: 'groups.xmpp.example.net',
    node: 'home-bakers',
    name: 'Home Bakers',
    description: 'Bread, starters, and what went wrong this time.',
    members: 186
  },
  {
    service: SVC,
    node: 'pixel-art',
    name: 'Pixel Art',
    description: 'Share sprites, ask for feedback, weekly theme on Mondays.',
    members: 93
  },
  {
    service: 'chat.example.org',
    node: 'bike-repair',
    name: 'Bike Repair',
    description: 'Help with brakes, gears, and wheels. Photos help.',
    members: 254
  }
];

// --- members ---------------------------------------------------------

function mem(p: Person, affiliation: MemberItem['affiliation']): MemberItem {
  // A room gives the moderator role to its owners and admins.
  const role = affiliation === 'owner' || affiliation === 'admin' ? 'Moderator' : null;
  return {
    id: p.address,
    name: p.name,
    role,
    affiliation,
    show: p.show,
    status: p.status,
    online: p.online,
    avatar: p.avatar
  };
}

export const members: Record<string, MemberItem[]> = {
  [K.lantern]: [
    mem(P.priya, 'owner'),
    mem(P.maya, 'admin'),
    mem(P.theo, 'admin'),
    mem(P.kenji, 'member'),
    mem(P.ada, 'member'),
    mem(P.jules, 'member'),
    mem(P.sam, 'member'),
    mem(P.noor, 'member'),
    mem(P.luis, 'member'),
    mem(P.wren, 'member')
  ],
  [K.darkroom]: [
    mem(P.noor, 'owner'),
    mem(P.maya, 'member'),
    mem(P.jules, 'member'),
    mem(P.wren, 'member'),
    mem(P.luis, 'member')
  ],
  [K.crag]: [
    mem(P.sam, 'owner'),
    mem(P.theo, 'admin'),
    mem(P.maya, 'member'),
    mem(P.wren, 'member')
  ],
  [K.readers]: [
    mem(P.ada, 'owner'),
    mem(P.priya, 'member'),
    mem(P.maya, 'member'),
    mem(P.jules, 'member')
  ],
  [K.sandbox]: [
    mem(P.maya, 'owner'),
    mem(P.theo, 'admin'),
    mem(P.kenji, 'member'),
    mem(P.priya, 'member')
  ],
  // The group chat on the home list keeps its members under the home key.
  home: [mem(P.maya, 'member'), mem(P.priya, 'member'), mem(P.theo, 'member'), mem(P.kenji, 'member')]
};

// --- timelines -------------------------------------------------------

const DAY = 86_400_000;
const startOfToday = new Date().setHours(0, 0, 0, 0);
/** Milliseconds for a clock time some days ago. */
function at(daysAgo: number, h: number, m: number): number {
  return startOfToday - daysAgo * DAY + (h * 60 + m) * 60_000;
}

type Draft = Partial<TimelineItem> & { id: string; who: string; body: string; ts: number };

function build(drafts: Draft[]): TimelineItem[] {
  const out: TimelineItem[] = [];
  for (const draft of drafts) {
    const { who, ts, ...d } = draft;
    const p = P[who];
    const sender = p.address;
    const prev = out[out.length - 1];
    out.push({
      sender,
      senderName: p.name,
      avatar: p.avatar,
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

function png(url: string, name: string, size: number, width: number, height: number) {
  return { url, name, mime: 'image/png', size, width, height };
}

/** A plain gradient picture as a data URI, for the GIF samples and the Sandbox. */
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

const NOTES_URL = 'https://lanternworks.example/notes/0-14';

// The hero conversation. The screenshots show it.
const playtest = build([
  {
    id: 'p1',
    who: 'priya',
    ts: at(0, 9, 12),
    body: 'Build 0.14.2 is up on the playtest branch. New save system, controller remapping, and the lighthouse level is finally in.'
  },
  { id: 'p2', who: 'priya', ts: at(0, 9, 12), body: 'Please break it 🙏' },
  { id: 'p3', who: 'theo', ts: at(0, 9, 20), body: 'downloading' },
  {
    id: 'p4',
    who: 'jules',
    ts: at(0, 9, 47),
    body: 'Played the lighthouse twice. The fog at the top looks great, but I lost the rope prompt both times. It spawns behind the camera.',
    attachment: png(lighthousePng, 'lighthouse-fog.png', 1_184_000, 1280, 720)
  },
  {
    id: 'p5',
    who: 'priya',
    ts: at(0, 9, 51),
    body: "Good catch. The prompt is placed in world space, I'll pin it to the screen edge when it's off camera.",
    replyTo: {
      id: 'p4',
      senderName: P.jules.name,
      body: 'Played the lighthouse twice. The fog at the top looks great, but I lost the rope prompt both times. It spawns behind the camera.'
    }
  },
  {
    id: 'p6',
    who: 'kenji',
    ts: at(0, 9, 53),
    body:
      'something like this?\n```gdscript\nif not camera.is_position_in_frustum(prompt.global_position):\n    prompt.pin_to_edge(camera)\n```'
  },
  {
    id: 'p7',
    who: 'priya',
    ts: at(0, 9, 54),
    body: 'yes, almost exactly. Want to open a PR?',
    reactions: [{ emoji: '👍', count: 3, mine: true }]
  },
  { id: 'p8', who: 'kenji', ts: at(0, 9, 54), body: 'on it' },
  {
    id: 'p9',
    who: 'maya',
    ts: at(0, 10, 2),
    body: "@Theo can you check that old saves still load? I don't want to ship 0.14 if anyone loses progress."
  },
  {
    id: 'p10',
    who: 'theo',
    ts: at(0, 10, 5),
    body: "On it. Three saves from 0.13 so far, all load fine. The cloud one takes about 4 s, I'll profile it."
  },
  { id: 'p11', who: 'ada', ts: at(0, 10, 31), body: `Draft of the patch notes: ${NOTES_URL}` },
  {
    id: 'p12',
    who: 'sam',
    ts: at(0, 10, 40),
    body: "Read it. I'd lead with remapping, that's what people asked for most in the survey.",
    reactions: [{ emoji: '❤️', count: 2, mine: false }]
  },
  { id: 'p13', who: 'ada', ts: at(0, 10, 42), body: 'fair, swapping them' }
]);

const announcements = build([
  {
    id: 'a1',
    who: 'priya',
    ts: at(2, 16, 10),
    body: '0.14.2 is out on the playtest branch. Saves from 0.13 carry over.'
  },
  { id: 'a2', who: 'priya', ts: at(0, 8, 30), body: 'Playtest call is Friday at 17:00 UTC.' }
]);

const bugs = build([
  {
    id: 'b1',
    who: 'jules',
    ts: at(0, 8, 14),
    body: 'Controller remap screen: pressing B twice exits without saving.'
  },
  { id: 'b2', who: 'kenji', ts: at(0, 8, 40), body: "Repro'd. Fix is in 0.14.3." },
  { id: 'b3', who: 'noor', ts: at(0, 9, 2), body: 'Same on keyboard with Esc.' }
]);

const art = build([
  {
    id: 'r1',
    who: 'luis',
    ts: at(1, 21, 15),
    body: 'Background pass for the valley level. Too green?',
    attachment: png(hillsPng, 'hills.png', 412_000, 1200, 800)
  },
  { id: 'r2', who: 'ada', ts: at(1, 21, 32), body: 'A bit. Try pulling the far hills toward blue.' },
  { id: 'r3', who: 'luis', ts: at(1, 21, 40), body: "yeah that's better, will push tonight" }
]);

const showAndTell = build([
  {
    id: 's1',
    who: 'noor',
    ts: at(0, 8, 5),
    body: 'Portra 400, pushed one stop. Harbor at the end of the day.',
    attachment: png(harborPng, 'harbor-sunset.png', 968_000, 800, 1000)
  },
  { id: 's2', who: 'wren', ts: at(0, 8, 21), body: 'the color in the water 😮' },
  { id: 's3', who: 'noor', ts: at(0, 8, 24), body: 'Lab scan, no edits.' },
  { id: 's4', who: 'jules', ts: at(0, 9, 3), body: 'Which lab?' },
  { id: 's5', who: 'noor', ts: at(0, 9, 6), body: "The one on 3rd, they're slow but careful." }
]);

const sessions = build([
  { id: 'c1', who: 'sam', ts: at(0, 7, 50), body: 'Thursday as usual?' },
  {
    id: 'c2',
    who: 'theo',
    ts: at(0, 8, 2),
    body: "I'm in. My fingers are still recovering from Tuesday."
  },
  { id: 'c3', who: 'wren', ts: at(0, 8, 15), body: 'Bringing the new tape.' },
  { id: 'c4', who: 'sam', ts: at(0, 8, 17), body: '18:30 at the wall then.' }
]);

const thisMonth = build([
  { id: 'm1', who: 'ada', ts: at(1, 20, 4), body: 'Chapter 6 changed how I read the first five.' },
  { id: 'm2', who: 'priya', ts: at(1, 20, 30), body: 'Same. I went back to the Ekumen report.' },
  { id: 'm3', who: 'ada', ts: at(1, 20, 33), body: 'No spoilers past 8 please 🙂' }
]);

// The Sandbox: one of each kind of message, for work on the preview.
const SOON = Math.floor((at(0, 17, 0) + DAY) / 1000);
const sandbox = build([
  { id: 'x1', who: 'theo', ts: at(1, 15, 2), body: 'Test: a short message.' },
  {
    id: 'x2',
    who: 'kenji',
    ts: at(1, 15, 4),
    body: 'Test: a link with a preview. https://example.org/docs/save-format',
    edited: true
  },
  {
    id: 'x3',
    who: 'theo',
    ts: at(1, 15, 10),
    body: 'Test: a big preview and a direct image link. https://status.example.net/servers and https://cdn.example.net/screenshot.png'
  },
  {
    id: 'x4',
    who: 'priya',
    ts: at(1, 15, 20),
    body: 'Test: a sound file.',
    attachment: {
      url: tone(2, 440),
      name: 'menu-click.wav',
      mime: 'audio/wav',
      size: 16_044,
      width: null,
      height: null
    }
  },
  {
    id: 'x5',
    who: 'kenji',
    ts: at(1, 15, 22),
    body: 'Test: a video.',
    attachment: {
      url: rangeClip,
      name: 'walkthrough.mp4',
      mime: 'video/mp4',
      size: 26_631,
      width: 640,
      height: 360
    }
  },
  {
    id: 'x6',
    who: 'theo',
    ts: at(1, 15, 25),
    body: 'Test: a PDF file.',
    attachment: {
      url: 'file:///save-format.pdf',
      name: 'save-format-v2.pdf',
      mime: 'application/pdf',
      size: 184_320,
      width: null,
      height: null
    }
  },
  {
    id: 'x7',
    who: 'theo',
    ts: at(1, 15, 25),
    body: '',
    attachment: {
      url: pic(400, 560, '#0a655c', '#111316'),
      name: 'tall.png',
      mime: 'image/png',
      size: 64_000,
      width: 400,
      height: 560
    }
  },
  // A person who is not a contact. With "Load files from people who are not contacts" off,
  // the photo shows as a plain file link and the link preview does not show.
  {
    id: 'x8',
    who: 'mika',
    ts: at(0, 9, 30),
    body: 'Test: a message from someone who is not a contact. https://example.org/fan-wiki/lighthouse',
    attachment: {
      url: pic(480, 320, '#5b4bb7', '#16131f'),
      name: 'fan-art.png',
      mime: 'image/png',
      size: 96_000,
      width: 480,
      height: 320
    }
  },
  { id: 'x9', who: 'kenji', ts: at(0, 9, 40), body: 'This message was deleted.', retracted: true },
  { id: 'x10', who: 'kenji', ts: at(0, 9, 41), body: '/me runs the **tests** again' },
  { id: 'x11', who: 'maya', ts: at(0, 9, 45), body: 'Test: this message did not send.', status: 'failed' },
  {
    id: 'x12',
    who: 'theo',
    ts: at(0, 9, 50),
    body: '@Maya test: a mention of you.',
    mention: true
  },
  {
    id: 'x13',
    who: 'priya',
    ts: at(0, 10, 0),
    body:
      '# Heading\n' +
      `The call is <t:${SOON}:F>, which is <t:${SOON}:R>.\n` +
      '> A quote with **bold** text.\n' +
      '- First item\n' +
      '  - A nested item in *italics*\n' +
      '- Second item\n' +
      '-# Small text at the end. :wave:'
  },
  {
    id: 'x14',
    who: 'kenji',
    ts: at(0, 10, 5),
    body:
      'Test: a code block.\n```python\nfor save in saves:\n    if save.version < 14:\n        migrate(save)\n        print("migrated", save.id)\n```\n' +
      'And a spoiler: ||the answer is 42||. A named link: [the docs](https://example.org/docs).',
    reactions: [
      { emoji: '👍', count: 2, mine: false },
      { emoji: '👀', count: 1, mine: true }
    ]
  },
  {
    id: 'x15',
    who: 'priya',
    ts: at(0, 10, 10),
    body: 'Test: a link to a public space. xmpp:chat.foid.space?pubsub;action=subscribe;node=pixel-art'
  },
  {
    id: 'x16',
    who: 'theo',
    ts: at(0, 10, 12),
    body: 'Test: a link to a room. xmpp:speedrun@chat.foid.space?join'
  },
  { id: 'x17', who: 'kenji', ts: at(0, 10, 15), body: ':tada: :fire:' }
]);

/** Sample link previews for the browser preview, keyed by URL. There is no network. */
export const linkPreviews: Record<string, LinkPreview> = {
  [NOTES_URL]: {
    url: NOTES_URL,
    siteName: 'Lantern Works',
    title: 'Patch notes 0.14: The Lighthouse',
    description:
      'Controller remapping, a new save system, and a lighthouse that took us far too long.',
    image: patchNotesPng,
    imageWidth: 1200,
    imageHeight: 630
  },
  'https://example.org/docs/save-format': {
    url: 'https://example.org/docs/save-format',
    siteName: 'Example Docs',
    title: 'Save format, version 2',
    description:
      'Each save has a header with the version, then one block per level. Old saves are migrated when they load.',
    image: pic(160, 160, '#0a655c', '#111316'),
    imageWidth: 160,
    imageHeight: 160
  },
  'https://status.example.net/servers': {
    url: 'https://status.example.net/servers',
    siteName: 'Server Status',
    title: 'All servers are up',
    description: null,
    image: pic(1200, 630, '#4cc3b5', '#21252b'),
    imageWidth: 1200,
    imageHeight: 630
  },
  'https://cdn.example.net/screenshot.png': {
    url: 'https://cdn.example.net/screenshot.png',
    siteName: null,
    title: null,
    description: null,
    image: pic(640, 400, '#c47b0c', '#3a2c16'),
    imageWidth: null,
    imageHeight: null
  },
  'https://example.org/fan-wiki/lighthouse': {
    url: 'https://example.org/fan-wiki/lighthouse',
    siteName: 'Fan Wiki',
    title: 'The lighthouse level',
    description: 'How to reach the top, and where the rope is.',
    image: null,
    imageWidth: null,
    imageHeight: null
  }
};

/** Rooms that an xmpp: link can name and that the user has not joined. Keyed by address. */
export const xmppRooms: Record<string, { name: string; subject: string | null; occupants: number | null }> = {
  'speedrun@chat.foid.space': {
    name: 'speedrun',
    subject: 'Routes and times for the Lantern Works games',
    occupants: 14
  }
};

export const timelines: Record<string, TimelineItem[]> = {
  [room(K.lantern, 'announcements')]: announcements,
  [room(K.lantern, 'general')]: [],
  [room(K.lantern, 'playtest')]: playtest,
  [room(K.lantern, 'bugs')]: bugs,
  [room(K.lantern, 'art')]: art,
  [room(K.darkroom, 'show-and-tell')]: showAndTell,
  [room(K.crag, 'sessions')]: sessions,
  [room(K.readers, 'this-month')]: thisMonth,
  [room(K.sandbox, 'general')]: sandbox,
  [P.theo.address]: build([
    { id: 'd1', who: 'theo', ts: at(0, 10, 50), body: 'lunch after the playtest call?' },
    { id: 'd2', who: 'maya', ts: at(0, 10, 52), body: 'Yes. Ramen place?' },
    { id: 'd3', who: 'theo', ts: at(0, 10, 53), body: 'perfect' }
  ]),
  [LANTERN_CORE]: build([
    { id: 'l1', who: 'kenji', ts: at(0, 10, 20), body: 'PR is up for the rope prompt.' },
    { id: 'l2', who: 'priya', ts: at(0, 10, 24), body: 'Reviewing after standup.' }
  ]),
  [P.priya.address]: build([
    { id: 'q1', who: 'maya', ts: at(1, 17, 40), body: 'Can you send me the build notes when they are ready?' },
    { id: 'q2', who: 'priya', ts: at(1, 17, 52), body: 'Will do, tomorrow morning.' }
  ]),
  [P.noor.address]: build([
    { id: 'n1', who: 'noor', ts: at(6, 19, 2), body: 'Thanks for the invite to the playtest.' },
    { id: 'n2', who: 'maya', ts: at(6, 19, 10), body: 'Glad you could join. Bugs go in #bugs.' }
  ]),
  [P.ada.address]: build([
    { id: 'e1', who: 'ada', ts: at(4, 11, 15), body: 'Patch notes draft by Thursday, ok?' },
    { id: 'e2', who: 'maya', ts: at(4, 11, 20), body: 'Ok, thanks.' }
  ]),
  [P.jules.address]: build([
    { id: 'j1', who: 'maya', ts: at(9, 14, 0), body: 'Can you test the lighthouse level this week?' },
    { id: 'j2', who: 'jules', ts: at(9, 15, 30), body: 'Sure, send me the build.' }
  ])
};

/** Read state: the first unread message per channel, for the "new" divider. */
export const firstUnread: Record<string, string> = {
  [room(K.lantern, 'playtest')]: 'p11',
  [P.theo.address]: 'd3'
};

export const typing: Record<string, string[]> = {
  [room(K.lantern, 'playtest')]: [P.kenji.name]
};

// --- contacts --------------------------------------------------------

function contact(p: Person, daysAgo: number | null = null): ContactItem {
  return {
    address: p.address,
    name: p.name,
    avatar: p.avatar,
    show: p.show,
    online: p.online,
    status: p.status,
    since: daysAgo === null ? null : at(daysAgo, 12, 0)
  };
}

export const contacts: ContactItem[] = [
  contact(P.priya, 420),
  contact(P.theo, 380),
  contact(P.jules, 95),
  contact(P.kenji, 300),
  contact(P.ada, 260),
  contact(P.noor, 40),
  contact(P.luis, 150),
  contact(P.sam, 210),
  contact(P.wren, 70)
];

export const incomingRequests: ContactItem[] = [contact(P.mika)];

export const outgoingRequests: ContactItem[] = [
  contact(person('ines@xmpp.example.net', 'Ines Duarte', null, null, false))
];

export const blockedContacts: ContactItem[] = [
  contact(person('promo@spam.example', 'promo@spam.example', null, null, false))
];

/** Contacts you and this address both have. Sample data only. */
export const sharedContacts: Record<string, string[]> = {
  [P.priya.address]: [P.theo.address, P.kenji.address, P.ada.address],
  [P.theo.address]: [P.priya.address, P.kenji.address, P.sam.address],
  [P.kenji.address]: [P.priya.address, P.theo.address],
  [P.ada.address]: [P.priya.address],
  [P.noor.address]: [P.jules.address, P.wren.address],
  [P.jules.address]: [P.noor.address, P.priya.address]
};

/** Sample GIF results for the picker in the preview. Real results come from KLIPY. */
export const gifs: import('$lib/chord').Gif[] = [
  ['wave', 'Wave', 320, 240, '#c47b0c', '#3a2c16'],
  ['nod', 'Nod', 240, 320, '#0a655c', '#111316'],
  ['thumbs-up', 'Thumbs up', 320, 180, '#f2a93b', '#21252b'],
  ['clap', 'Clap', 280, 280, '#4cc3b5', '#181b20'],
  ['coffee', 'Coffee', 320, 200, '#7a4a00', '#111316'],
  ['shrug', 'Shrug', 300, 220, '#f0676b', '#3a2c16'],
  ['cat', 'Cat typing', 240, 240, '#5bc46e', '#181b20'],
  ['thanks', 'Thanks', 320, 260, '#9aa0a8', '#21252b']
].map(([slug, title, w, h, a, b]) => {
  const file = { url: pic(w as number, h as number, a as string, b as string), width: w as number, height: h as number };
  return { slug: slug as string, title: title as string, preview: file, full: file };
});
