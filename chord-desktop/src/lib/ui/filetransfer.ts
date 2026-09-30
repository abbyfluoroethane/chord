// Pure helpers for the files that the user drops on the chat or pastes into the composer.
// A drop gives Rust a path (the webview drag-drop event). A paste gives the page bytes and
// no path, so the page sends the bytes to Rust (`uploadPasted`).

/** The biggest pasted file. Rust has the same cap (MAX_PASTE_BYTES in files.rs). */
export const MAX_PASTE_BYTES = 25 * 1024 * 1024;

/** What the window-level drag-drop event means for the chat. */
export type DropAction = 'show' | 'hide' | 'upload' | 'ignore';

/**
 * The action for a drag-drop event of the webview. The contacts page has no chat, so it
 * takes no file. `enter` and `over` show the drop hint, `leave` hides it, and `drop`
 * uploads.
 */
export function dropAction(type: string, showContacts: boolean): DropAction {
  if (showContacts) return type === 'leave' || type === 'drop' ? 'hide' : 'ignore';
  switch (type) {
    case 'enter':
    case 'over':
      return 'show';
    case 'leave':
      return 'hide';
    case 'drop':
      return 'upload';
    default:
      return 'ignore';
  }
}

/** The files of a paste event. Text pastes have none. */
export function pastedFiles(data: { files?: ArrayLike<File> } | null | undefined): File[] {
  return data?.files ? Array.from(data.files) : [];
}

/** Why a pasted file cannot go, or null if it can. */
export function pasteProblem(file: { size: number }): string | null {
  if (file.size === 0) return 'The pasted file is empty.';
  if (file.size > MAX_PASTE_BYTES) return 'The pasted file is over 25 MB.';
  return null;
}
