// The attachment tray: the pure logic. A paste, a drop or a file pick adds an item. Nothing
// uploads until the user sends. The store in tray.svelte.ts keeps the items and the preview
// URLs. This file has no state, so the tests can run it without a page.
import { MAX_PASTE_BYTES } from './filetransfer';
import { fileSize } from './format';

/** The most items that one message can carry. */
export const MAX_TRAY_ITEMS = 10;

export type TrayStatus = 'ready' | 'uploading' | 'failed';

export interface TrayItem {
  id: string;
  /** The bytes, for a paste or a pick. A dropped file has a path in the live app. */
  file: File | null;
  /** The path from a drop event. Only Rust reads it. */
  path: string | null;
  name: string;
  /** The size in bytes, or null when the page cannot know it (a dropped path). */
  size: number | null;
  mime: string;
  /** An object URL for the thumbnail of an image or a video, or null. */
  preview: string | null;
  status: TrayStatus;
  error: string | null;
}

let counter = 0;
const nextId = () => `tray-${Date.now().toString(36)}-${(counter++).toString(36)}`;

/** The base name of a path, for either kind of separator. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Why a file cannot join the tray, or null if it can. */
export function fileProblem(file: { name: string; size: number }): string | null {
  const name = file.name || 'The file';
  if (file.size === 0) return `${name} is empty.`;
  if (file.size > MAX_PASTE_BYTES) {
    return `${name} is too big (${fileSize(file.size)}). The limit is ${fileSize(MAX_PASTE_BYTES)}.`;
  }
  return null;
}

/** The kind of thumbnail that an item shows. */
export function previewKind(mime: string): 'image' | 'video' | null {
  if (mime.startsWith('image/')) return 'image';
  if (mime.startsWith('video/')) return 'video';
  return null;
}

/** A new tray item for a file. `preview` is the object URL of its thumbnail, if it has one. */
export function fileItem(file: File, preview: string | null): TrayItem {
  return {
    id: nextId(),
    file,
    path: null,
    name: file.name || 'file',
    size: file.size,
    mime: file.type || 'application/octet-stream',
    preview,
    status: 'ready',
    error: null
  };
}

/** A new tray item for a dropped path. */
export function pathItem(path: string): TrayItem {
  return {
    id: nextId(),
    file: null,
    path,
    name: baseName(path),
    size: null,
    mime: 'application/octet-stream',
    preview: null,
    status: 'ready',
    error: null
  };
}

/**
 * Pick the files that fit in the tray. `room` is the number of free places. A file that is
 * empty or too big, or that has no place, gives a problem text and does not join.
 */
export function admit(files: File[], room: number): { accepted: File[]; problems: string[] } {
  const accepted: File[] = [];
  const problems: string[] = [];
  for (const f of files) {
    const problem = fileProblem(f);
    if (problem) problems.push(problem);
    else if (accepted.length >= room) {
      problems.push(`A message can carry ${MAX_TRAY_ITEMS} files at most.`);
      break;
    } else accepted.push(f);
  }
  return { accepted, problems };
}

/** The items that a send takes: the ones that wait, and the ones that failed before. */
export function sendable(items: TrayItem[]): TrayItem[] {
  return items.filter((i) => i.status !== 'uploading');
}

export function isBusy(items: TrayItem[]): boolean {
  return items.some((i) => i.status === 'uploading');
}

export interface UploadHooks {
  /** Upload one item. Resolve to null on success, or to the error text. */
  upload: (item: TrayItem) => Promise<string | null>;
  start: (id: string) => void;
  fail: (id: string, error: string) => void;
  done: (id: string) => void;
}

/** Upload the items one at a time, in tray order. A failed item stays, and the next one goes on. */
export async function runUploads(items: TrayItem[], hooks: UploadHooks): Promise<number> {
  let failed = 0;
  for (const item of items) {
    hooks.start(item.id);
    let error: string | null;
    try {
      error = await hooks.upload(item);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
    if (error) {
      failed++;
      hooks.fail(item.id, error);
    } else hooks.done(item.id);
  }
  return failed;
}
