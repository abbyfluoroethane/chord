// The keymap: every global shortcut in one list, and the code that matches a key event.
// The Ctrl+/ modal and the Keybinds page read this list through `shortcuts.ts`.
// Components do not match keys on their own for global shortcuts.

export type Action =
  | 'switcher'
  | 'settings'
  | 'shortcuts'
  | 'channelPrev'
  | 'channelNext'
  | 'unreadPrev'
  | 'unreadNext'
  | 'spacePrev'
  | 'spaceNext'
  | 'markRead'
  | 'markAllRead'
  | 'oldestUnread'
  | 'present'
  | 'pageUp'
  | 'pageDown'
  | 'reply'
  | 'emoji'
  | 'gif'
  | 'upload'
  | 'search'
  | 'pins'
  | 'members';

/** One key press. `key` is a letter in lower case, a symbol, or an `event.key` name. */
export interface Combo {
  key: string;
  /** Cmd on a Mac, Ctrl on other systems. */
  mod?: boolean;
  /** The Ctrl key on every system. */
  ctrl?: boolean;
  shift?: boolean;
  alt?: boolean;
}

export type Group = 'Navigation' | 'Messages' | 'Chat' | 'App';

export interface Binding {
  action: Action;
  group: Group;
  what: string;
  /** The first combo shows in the lists. The others are alternates. */
  combos: Combo[];
}

/** The part of a keyboard event that the matching reads. */
export interface KeyLike {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

export const bindings: Binding[] = [
  {
    action: 'switcher',
    group: 'Navigation',
    what: 'Jump to a space, channel, or DM',
    combos: [{ key: 'k', mod: true }]
  },
  {
    action: 'channelPrev',
    group: 'Navigation',
    what: 'Previous channel',
    combos: [{ key: 'ArrowUp', alt: true }]
  },
  {
    action: 'channelNext',
    group: 'Navigation',
    what: 'Next channel',
    combos: [{ key: 'ArrowDown', alt: true }]
  },
  {
    action: 'unreadPrev',
    group: 'Navigation',
    what: 'Previous channel with unread messages',
    combos: [{ key: 'ArrowUp', alt: true, shift: true }]
  },
  {
    action: 'unreadNext',
    group: 'Navigation',
    what: 'Next channel with unread messages',
    combos: [{ key: 'ArrowDown', alt: true, shift: true }]
  },
  {
    action: 'spacePrev',
    group: 'Navigation',
    what: 'Previous space',
    combos: [
      { key: 'ArrowUp', mod: true, alt: true },
      { key: 'Tab', ctrl: true, shift: true }
    ]
  },
  {
    action: 'spaceNext',
    group: 'Navigation',
    what: 'Next space',
    combos: [
      { key: 'ArrowDown', mod: true, alt: true },
      { key: 'Tab', ctrl: true }
    ]
  },
  {
    action: 'markRead',
    group: 'Messages',
    what: 'Mark this channel as read (not while you type)',
    combos: [{ key: 'Escape' }]
  },
  {
    action: 'markAllRead',
    group: 'Messages',
    what: 'Mark every channel as read',
    combos: [{ key: 'a', mod: true, shift: true }]
  },
  {
    action: 'oldestUnread',
    group: 'Messages',
    what: 'Jump to the oldest unread message',
    combos: [{ key: 'PageUp', shift: true }]
  },
  {
    action: 'present',
    group: 'Messages',
    what: 'Jump to the newest message',
    combos: [{ key: 'PageDown', shift: true }]
  },
  { action: 'pageUp', group: 'Messages', what: 'Scroll the messages up', combos: [{ key: 'PageUp' }] },
  {
    action: 'pageDown',
    group: 'Messages',
    what: 'Scroll the messages down',
    combos: [{ key: 'PageDown' }]
  },
  {
    action: 'reply',
    group: 'Messages',
    what: 'Reply to the selected message, or to the last one',
    combos: [{ key: 'r', alt: true }]
  },
  { action: 'emoji', group: 'Chat', what: 'Open the emoji picker', combos: [{ key: 'e', mod: true }] },
  { action: 'gif', group: 'Chat', what: 'Open the GIF picker', combos: [{ key: 'g', mod: true }] },
  {
    action: 'upload',
    group: 'Chat',
    what: 'Upload a file',
    combos: [{ key: 'u', mod: true, shift: true }]
  },
  { action: 'search', group: 'Chat', what: 'Search in this chat', combos: [{ key: 'f', mod: true }] },
  { action: 'pins', group: 'Chat', what: 'Show the pinned messages', combos: [{ key: 'p', mod: true }] },
  {
    action: 'members',
    group: 'Chat',
    what: 'Show or hide the member list',
    combos: [{ key: 'u', mod: true }]
  },
  { action: 'settings', group: 'App', what: 'Open settings', combos: [{ key: ',', mod: true }] },
  { action: 'shortcuts', group: 'App', what: 'Show this list', combos: [{ key: '/', mod: true }] }
];

/** The `event.code` of the symbol keys. A layout that types no Latin letters needs it. */
const SYMBOL_CODES: Record<string, string> = { ',': 'Comma', '/': 'Slash', '.': 'Period' };

const isLetter = (k: string) => k.length === 1 && /[a-z]/.test(k);
/** A key that is not a Latin one: a Cyrillic or Greek letter, or a symbol that Option types. */
const isForeign = (k: string) => k.length === 1 && k.charCodeAt(0) > 0x7f;

/** True when the key event is the combo. `mac` says whether Cmd stands for the mod key. */
export function matches(e: KeyLike, c: Combo, mac: boolean): boolean {
  const wantMeta = !!c.mod && mac;
  const wantCtrl = !!c.ctrl || (!!c.mod && !mac);
  if (e.metaKey !== wantMeta || e.ctrlKey !== wantCtrl) return false;
  if (e.altKey !== !!c.alt) return false;
  const named = c.key.length > 1;
  const symbol = !named && !isLetter(c.key);
  // A symbol can need Shift on some layouts (the slash on a German layout). Letters and
  // named keys must match Shift exactly.
  if (!symbol && e.shiftKey !== !!c.shift) return false;
  if (symbol && c.shift && !e.shiftKey) return false;

  if (named) return e.key === c.key;
  const typed = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (typed === c.key) return true;
  // The key typed is not Latin. Option on a Mac does this to every letter, and so does a
  // Cyrillic layout. The physical key is the best guide then.
  if (isForeign(e.key)) {
    return e.code === (isLetter(c.key) ? `Key${c.key.toUpperCase()}` : SYMBOL_CODES[c.key]);
  }
  return false;
}

/** The binding for a key event, or null. */
export function findBinding(e: KeyLike, mac: boolean, list: Binding[] = bindings): Binding | null {
  for (const b of list) if (b.combos.some((c) => matches(e, c, mac))) return b;
  return null;
}

const KEY_NAMES: Record<string, string> = {
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Escape: 'Esc',
  PageUp: 'Page Up',
  PageDown: 'Page Down'
};

/** The key names of a combo, for the lists. */
export function comboLabel(c: Combo, mac: boolean): string[] {
  const out: string[] = [];
  if (c.ctrl || (c.mod && !mac)) out.push('Ctrl');
  if (c.mod && mac) out.push('Cmd');
  if (c.alt) out.push(mac ? 'Option' : 'Alt');
  if (c.shift) out.push('Shift');
  out.push(KEY_NAMES[c.key] ?? (c.key.length === 1 ? c.key.toUpperCase() : c.key));
  return out;
}

/** The item after (or before) `current` in `list`. The list wraps. Null for an empty list. */
export function neighbor<T>(list: T[], current: T, dir: 1 | -1): T | null {
  if (!list.length) return null;
  const i = list.indexOf(current);
  if (i < 0) return dir === 1 ? list[0] : list[list.length - 1];
  return list[(i + dir + list.length) % list.length];
}

/** The order of the spaces in the rail: folders open up, and unknown ids drop out. */
export function railOrder(
  layout: ({ kind: 'circle'; id: string } | { kind: 'folder'; circles: string[] })[],
  known: string[]
): string[] {
  const have = new Set(known);
  const out: string[] = [];
  for (const entry of layout) {
    for (const id of entry.kind === 'circle' ? [entry.id] : entry.circles) {
      if (have.has(id) && !out.includes(id)) out.push(id);
    }
  }
  // A space that the layout does not list yet goes last.
  for (const id of known) if (!out.includes(id)) out.push(id);
  return out;
}
