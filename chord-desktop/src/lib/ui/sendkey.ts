// Which key sends a message. The setting "Send with" picks Enter or Ctrl+Enter (Cmd+Enter
// on a Mac). With Enter, Shift+Enter adds a line. With the other, Enter adds a line.
import type { SendKey } from './prefsdata';

export function isSendKey(
  e: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'ctrlKey' | 'metaKey' | 'isComposing'>,
  mode: SendKey
): boolean {
  if (e.key !== 'Enter' || e.isComposing) return false;
  return mode === 'enter' ? !e.shiftKey : e.ctrlKey || e.metaKey;
}
