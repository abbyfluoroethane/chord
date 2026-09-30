// When the open chat counts as read. Discord reads a chat when its newest message is on
// screen in a window that has focus. The "new" line stays until the chat closes.

export interface ReadView {
  /** The list shows its bottom (the newest message). */
  atBottom: boolean;
  /** The page is visible (not a hidden window or tab). */
  visible: boolean;
  /** The window has the keyboard focus. */
  focused: boolean;
  /** Unread messages and mentions of the open chat. */
  unread: number;
  mentions: number;
}

/** True when the open chat should be marked as read now. */
export function shouldReadAtBottom(v: ReadView): boolean {
  return v.atBottom && v.visible && v.focused && (v.unread > 0 || v.mentions > 0);
}

/** True when the "new" line is on screen or below the top of the list: the reader saw it. */
export function newLineSeen(lineTop: number | null, listTop: number): boolean {
  return lineTop !== null && lineTop >= listTop;
}
