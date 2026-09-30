// The shortcut list. The Ctrl+/ modal and the Keybinds page share it.

const mac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
export const mod = mac ? 'Cmd' : 'Ctrl';

export interface Shortcut {
  keys: string[];
  what: string;
}

export const shortcuts: Shortcut[] = [
  { keys: [mod, 'K'], what: 'Jump to a space, channel, or DM' },
  { keys: ['Alt', 'Up'], what: 'Previous channel' },
  { keys: ['Alt', 'Down'], what: 'Next channel' },
  { keys: ['Alt', 'Shift', 'Up'], what: 'Previous channel with unread messages' },
  { keys: ['Alt', 'Shift', 'Down'], what: 'Next channel with unread messages' },
  { keys: ['Esc'], what: 'Mark this channel as read' },
  { keys: ['Up'], what: 'Edit your last message (in an empty composer)' },
  { keys: ['Enter'], what: 'Send a message' },
  { keys: ['Shift', 'Enter'], what: 'Start a new line' },
  { keys: [mod, ','], what: 'Open settings' },
  { keys: [mod, '/'], what: 'Show this list' }
];
