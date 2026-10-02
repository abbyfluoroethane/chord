// The shortcut list. The Ctrl+/ modal and the Keybinds page share it.
// The global shortcuts come from `keymap.ts`. The rows at the end belong to one component.

import { bindings, comboLabel, type Group } from './keymap';

export const mac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
export const mod = mac ? 'Cmd' : 'Ctrl';

export type ShortcutGroup = Group | 'Composer';

export interface Shortcut {
  keys: string[];
  /** Other key sets that do the same. */
  alternates?: string[][];
  what: string;
  group: ShortcutGroup;
}

/** Shortcuts that a component handles on its own, inside a text field. */
const local: Shortcut[] = [
  { keys: ['Enter'], what: 'Send a message', group: 'Composer' },
  { keys: ['Shift', 'Enter'], what: 'Start a new line', group: 'Composer' },
  { keys: ['Up'], what: 'Edit your last message (in an empty composer)', group: 'Composer' },
  { keys: ['Any letter'], what: 'Start to type a message from anywhere in the chat', group: 'Composer' }
];

export const shortcutGroups: ShortcutGroup[] = ['Navigation', 'Messages', 'Chat', 'Composer', 'App'];

export const shortcuts: Shortcut[] = [
  ...bindings.map((b) => ({
    keys: comboLabel(b.combos[0], mac),
    alternates: b.combos.slice(1).map((c) => comboLabel(c, mac)),
    what: b.what,
    group: b.group
  })),
  ...local
];
