// Sample data for the UI shell. The bridge replaces all of this later.
// The story (people, spaces, messages) is in docs/brand/showcase/README.md. The Android
// showcase uses the same story, so keep the two in step. The art in ./showcase is a copy of
// the art in docs/brand/showcase.
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
import kenjiPng from './showcase/kenji.png';
import luisPng from './showcase/luis.png';
import mayaPng from './showcase/maya.png';
import noorPng from './showcase/noor.png';
import priyaPng from './showcase/priya.png';
import theoPng from './showcase/theo.png';
import basementPng from './showcase/basement.png';
import nightOwlsPng from './showcase/night-owls.png';
import darkroomPng from './showcase/darkroom.png';
import cragPng from './showcase/crag.png';
import harborPng from './showcase/harbor-sunset.png';
import hillsPng from './showcase/hills.png';
import wannacryJpg from './showcase/wannacry.jpg';

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

/** Everyone in the story, keyed by nickname. The art file names come from an older cast. */
const P: Record<string, Person> = {
  nina: person('nina@chat.foid.space', 'nina', mayaPng, 'chat', true),
  marco: person('marco@chat.foid.space', 'marco', theoPng, 'chat', true),
  jess: person('jess@chat.foid.space', 'jess', priyaPng, 'chat', true),
  theo: person('theo@chat.foid.space', 'theo', luisPng, 'away', true, 'at work'),
  sam: person('sam@chat.foid.space', 'sam', null, 'chat', true),
  kai: person('kai@xmpp.example.net', 'kai', noorPng, 'chat', true),
  lena: person('lena@chat.foid.space', 'lena', adaPng, 'dnd', true),
  rory: person('rory@chat.foid.space', 'rory', kenjiPng, null, false),
  ben: person('ben@chat.foid.space', 'ben', null, null, false),
  // Not a contact: she sent a contact request. The Sandbox uses her for a stranger's message.
  mika: person('mika@xmpp.example.net', 'mika', null, null, false)
};

export const me: Me = {
  address: P.nina.address,
  name: P.nina.name,
  avatar: P.nina.avatar,
  show: P.nina.show,
  status: P.nina.status
};

// --- spaces and channels ---------------------------------------------

export const spaces: SpaceItem[] = [
  { service: SVC, node: 'basement', name: 'basement', avatar: basementPng },
  { service: SVC, node: 'night-owls', name: 'night owls', avatar: nightOwlsPng },
  { service: SVC, node: 'film-club', name: 'film club', avatar: darkroomPng },
  { service: SVC, node: 'climbing', name: 'climbing', avatar: cragPng },
  { service: SVC, node: 'sandbox', name: 'Sandbox', avatar: null }
];

const K = {
  basement: spaceKey(spaces[0]),
  owls: spaceKey(spaces[1]),
  film: spaceKey(spaces[2]),
  climbing: spaceKey(spaces[3]),
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

const SUNDAY_DINNER = 'sunday-dinner@conference.chat.foid.space';

export const channels: ChannelItem[] = [
  dm(P.jess, { unread: 1 }),
  // A room outside any space: a group chat among the DMs.
  {
    jid: SUNDAY_DINNER,
    name: 'sunday dinner',
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
  dm(P.marco),
  dm(P.kai),
  dm(P.lena),

  ch(K.basement, 'general', 'general', { unread: 5 }),
  ch(K.basement, 'music', 'music', { unread: 2, topic: 'post songs' }),
  ch(K.basement, 'pics', 'pics', { unread: 4 }),
  ch(K.basement, 'games', 'games'),
  ch(K.basement, 'memes', 'memes', { muted: true, unread: 23 }),

  ch(K.owls, 'lfg', 'lfg', { unread: 4 }),
  ch(K.owls, 'clips', 'clips'),

  ch(K.film, 'show-and-tell', 'show-and-tell', { unread: 3 }),
  ch(K.film, 'general', 'general'),

  ch(K.climbing, 'sessions', 'sessions', { unread: 4, topic: 'tue / thu / sat' }),

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
  [K.basement]: [
    mem(P.theo, 'owner'),
    mem(P.nina, 'admin'),
    mem(P.marco, 'member'),
    mem(P.jess, 'member'),
    mem(P.sam, 'member'),
    mem(P.kai, 'member'),
    mem(P.lena, 'member'),
    mem(P.rory, 'member'),
    mem(P.ben, 'member')
  ],
  [K.owls]: [
    mem(P.marco, 'owner'),
    mem(P.nina, 'member'),
    mem(P.rory, 'member'),
    mem(P.lena, 'member'),
    mem(P.ben, 'member')
  ],
  [K.film]: [mem(P.lena, 'owner'), mem(P.nina, 'member'), mem(P.kai, 'member'), mem(P.theo, 'member')],
  [K.climbing]: [mem(P.sam, 'owner'), mem(P.nina, 'member'), mem(P.jess, 'member')],
  [K.sandbox]: [
    mem(P.nina, 'owner'),
    mem(P.theo, 'admin'),
    mem(P.marco, 'member'),
    mem(P.jess, 'member')
  ],
  // The group chat on the home list keeps its members under the home key.
  home: [mem(P.nina, 'member'), mem(P.marco, 'member'), mem(P.theo, 'member'), mem(P.jess, 'member')]
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

const SONG_URL = 'https://www.youtube.com/watch?v=Ob_EDY9Eiis';

// The hero conversation. The screenshots show it.
const music = build([
  { id: 'm1', who: 'nina', ts: at(0, 21, 4), body: 'new ninajirachi is out' },
  {
    id: 'm2',
    who: 'nina',
    ts: at(0, 21, 4),
    body: SONG_URL,
    reactions: [{ emoji: '🔥', count: 4, mine: true }]
  },
  { id: 'm3', who: 'marco', ts: at(0, 21, 5), body: 'WITH PORTER??' },
  { id: 'm4', who: 'nina', ts: at(0, 21, 5), body: 'with porter' },
  { id: 'm5', who: 'jess', ts: at(0, 21, 7), body: 'ok this goes so hard' },
  { id: 'm6', who: 'marco', ts: at(0, 21, 9), body: 'the drop is insane' },
  {
    id: 'm7',
    who: 'jess',
    ts: at(0, 21, 10),
    body: 'is this the one from coachella',
    replyTo: { id: 'm6', senderName: P.marco.name, body: 'the drop is insane' }
  },
  { id: 'm8', who: 'nina', ts: at(0, 21, 10), body: 'yeah he came out for it in april' },
  { id: 'm9', who: 'theo', ts: at(0, 21, 14), body: 'adding it to the car playlist' },
  { id: 'm10', who: 'kai', ts: at(0, 21, 15), body: 'is she touring this year' }
]);

const general = build([
  { id: 'g1', who: 'theo', ts: at(0, 18, 2), body: 'who left a hoodie at mine' },
  { id: 'g2', who: 'jess', ts: at(0, 18, 10), body: 'grey one?' },
  { id: 'g3', who: 'theo', ts: at(0, 18, 11), body: 'yeah' },
  { id: 'g4', who: 'jess', ts: at(0, 18, 12), body: "thats sam's" },
  { id: 'g5', who: 'sam', ts: at(0, 18, 40), body: 'i was wondering where that went' }
]);

const pics = build([
  {
    id: 'i1',
    who: 'theo',
    ts: at(0, 19, 31),
    body: 'sunset from the ferry',
    attachment: png(harborPng, 'harbor-sunset.png', 968_000, 800, 1000)
  },
  { id: 'i2', who: 'nina', ts: at(0, 19, 40), body: 'wait where is this' },
  { id: 'i3', who: 'theo', ts: at(0, 19, 42), body: 'coming back from the island' },
  { id: 'i4', who: 'jess', ts: at(0, 19, 50), body: 'jealous' }
]);

const lfg = build([
  { id: 'f1', who: 'marco', ts: at(0, 20, 30), body: 'lobby in 10?' },
  { id: 'f2', who: 'rory', ts: at(0, 20, 31), body: 'give me 15 im eating' },
  { id: 'f3', who: 'marco', ts: at(0, 20, 31), body: 'ok 15' },
  { id: 'f4', who: 'lena', ts: at(0, 20, 35), body: 'im in' }
]);

const showAndTell = build([
  {
    id: 's1',
    who: 'lena',
    ts: at(1, 16, 20),
    body: 'first roll from the new camera',
    attachment: png(hillsPng, 'hills.png', 412_000, 1200, 800)
  },
  { id: 's2', who: 'kai', ts: at(1, 17, 2), body: 'the haze in the back 👌' },
  { id: 's3', who: 'lena', ts: at(1, 17, 10), body: 'it was so foggy that morning' }
]);

const sessions = build([
  { id: 'c1', who: 'sam', ts: at(0, 12, 15), body: 'thursday?' },
  { id: 'c2', who: 'jess', ts: at(0, 12, 40), body: 'cant, work' },
  { id: 'c3', who: 'sam', ts: at(0, 12, 41), body: 'saturday then' },
  { id: 'c4', who: 'jess', ts: at(0, 12, 50), body: 'saturday works' }
]);

// The Sandbox: one of each kind of message, for work on the preview.
const SOON = Math.floor((at(0, 17, 0) + DAY) / 1000);
const sandbox = build([
  { id: 'x1', who: 'theo', ts: at(1, 15, 2), body: 'Test: a short message.' },
  {
    id: 'x2',
    who: 'marco',
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
    who: 'jess',
    ts: at(1, 15, 20),
    body: 'Test: a sound file.',
    attachment: {
      url: tone(2, 440),
      name: 'voice-note.wav',
      mime: 'audio/wav',
      size: 16_044,
      width: null,
      height: null
    }
  },
  {
    id: 'x5',
    who: 'marco',
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
      url: 'file:///tickets.pdf',
      name: 'tickets.pdf',
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
    body: 'Test: a message from someone who is not a contact. https://example.org/wiki/test-page',
    attachment: {
      url: pic(480, 320, '#5b4bb7', '#16131f'),
      name: 'photo.png',
      mime: 'image/png',
      size: 96_000,
      width: 480,
      height: 320
    }
  },
  { id: 'x9', who: 'marco', ts: at(0, 9, 40), body: 'This message was deleted.', retracted: true },
  { id: 'x10', who: 'marco', ts: at(0, 9, 41), body: '/me checks the **logs** again' },
  { id: 'x11', who: 'nina', ts: at(0, 9, 45), body: 'Test: this message did not send.', status: 'failed' },
  {
    id: 'x12',
    who: 'theo',
    ts: at(0, 9, 50),
    body: '@nina test: a mention of you.',
    mention: true
  },
  {
    id: 'x13',
    who: 'jess',
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
    who: 'marco',
    ts: at(0, 10, 5),
    body:
      'Test: a code block.\n```python\nfor song in playlist:\n    if song.plays > 100:\n        favourites.append(song)\n        print("added", song.title)\n```\n' +
      'And a spoiler: ||the answer is 42||. A named link: [the docs](https://example.org/docs).',
    reactions: [
      { emoji: '👍', count: 2, mine: false },
      { emoji: '👀', count: 1, mine: true }
    ]
  },
  {
    id: 'x15',
    who: 'jess',
    ts: at(0, 10, 10),
    body: 'Test: a link to a public space. xmpp:chat.foid.space?pubsub;action=subscribe;node=pixel-art'
  },
  {
    id: 'x16',
    who: 'theo',
    ts: at(0, 10, 12),
    body: 'Test: a link to a room. xmpp:speedrun@chat.foid.space?join'
  },
  { id: 'x17', who: 'marco', ts: at(0, 10, 15), body: ':tada: :fire:' }
]);

/** Sample link previews for the browser preview, keyed by URL. There is no network. */
export const linkPreviews: Record<string, LinkPreview> = {
  [SONG_URL]: {
    url: SONG_URL,
    siteName: 'YouTube',
    title: 'Ninajirachi & Porter Robinson - WannaCry [Official Visualiser]',
    description: 'Ninajirachi',
    image: wannacryJpg,
    imageWidth: 1280,
    imageHeight: 720
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
  'https://example.org/wiki/test-page': {
    url: 'https://example.org/wiki/test-page',
    siteName: 'Example Wiki',
    title: 'Test page',
    description: 'A page for link preview tests.',
    image: null,
    imageWidth: null,
    imageHeight: null
  }
};

/** Rooms that an xmpp: link can name and that the user has not joined. Keyed by address. */
export const xmppRooms: Record<string, { name: string; subject: string | null; occupants: number | null }> = {
  'speedrun@chat.foid.space': {
    name: 'speedrun',
    subject: 'Routes and times',
    occupants: 14
  }
};

export const timelines: Record<string, TimelineItem[]> = {
  [room(K.basement, 'general')]: general,
  [room(K.basement, 'music')]: music,
  [room(K.basement, 'pics')]: pics,
  [room(K.owls, 'lfg')]: lfg,
  [room(K.film, 'show-and-tell')]: showAndTell,
  [room(K.climbing, 'sessions')]: sessions,
  [room(K.sandbox, 'general')]: sandbox,
  [P.jess.address]: build([
    { id: 'd1', who: 'jess', ts: at(0, 17, 20), body: 'are you going to the show on the 18th' },
    { id: 'd2', who: 'nina', ts: at(0, 17, 31), body: 'if i can get off work' },
    { id: 'd3', who: 'jess', ts: at(0, 17, 32), body: 'ill grab 2 just in case' }
  ]),
  [SUNDAY_DINNER]: build([
    { id: 'l1', who: 'marco', ts: at(0, 16, 5), body: 'still on for sunday?' },
    { id: 'l2', who: 'theo', ts: at(0, 16, 20), body: 'yes my place' },
    { id: 'l3', who: 'theo', ts: at(0, 16, 20), body: 'bring chairs if you have them' }
  ]),
  [P.marco.address]: build([
    { id: 'q1', who: 'marco', ts: at(2, 22, 10), body: 'you up for games tmrw' },
    { id: 'q2', who: 'nina', ts: at(2, 22, 30), body: 'yeah after 8' }
  ]),
  [P.kai.address]: build([
    { id: 'n1', who: 'kai', ts: at(5, 13, 2), body: 'sent you the photos' },
    { id: 'n2', who: 'nina', ts: at(5, 13, 40), body: 'got them thanks' }
  ]),
  [P.lena.address]: build([
    { id: 'e1', who: 'nina', ts: at(8, 10, 15), body: 'can i borrow your 50mm on saturday' },
    { id: 'e2', who: 'lena', ts: at(8, 11, 0), body: 'sure' }
  ])
};

/** Read state: the first unread message per channel, for the "new" divider. */
export const firstUnread: Record<string, string> = {
  [room(K.basement, 'music')]: 'm9',
  [P.jess.address]: 'd3'
};

export const typing: Record<string, string[]> = {
  [room(K.basement, 'music')]: [P.marco.name]
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
  contact(P.marco, 900),
  contact(P.jess, 870),
  contact(P.theo, 820),
  contact(P.sam, 600),
  contact(P.kai, 140),
  contact(P.lena, 330),
  contact(P.rory, 410),
  contact(P.ben, 75)
];

export const incomingRequests: ContactItem[] = [contact(P.mika)];

export const outgoingRequests: ContactItem[] = [
  contact(person('ines@xmpp.example.net', 'ines', null, null, false))
];

export const blockedContacts: ContactItem[] = [
  contact(person('promo@spam.example', 'promo@spam.example', null, null, false))
];

/** Contacts you and this address both have. Sample data only. */
export const sharedContacts: Record<string, string[]> = {
  [P.jess.address]: [P.marco.address, P.theo.address, P.sam.address],
  [P.marco.address]: [P.jess.address, P.theo.address, P.rory.address],
  [P.theo.address]: [P.marco.address, P.jess.address],
  [P.kai.address]: [P.lena.address],
  [P.lena.address]: [P.kai.address, P.rory.address]
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
