import { convertFileSrc } from '@tauri-apps/api/core';

/** The scheme that Rust registers (src-tauri/src/avatars.rs). */
export const AVATAR_SCHEME = 'chord-avatar';

/**
 * The `<img src>` for an avatar. `key` is the avatar hash of a view item
 * (`TimelineItem.avatar`, `MemberItem.avatar`, `SpaceItem.avatar`), or an owner: a JID,
 * or `service/node` for a space. A hash never changes, so the browser caches it.
 * It gives a 404 when no image is stored: hide the `<img>` on its error event.
 */
export function avatarUrl(key: string): string {
  return convertFileSrc(key, AVATAR_SCHEME);
}

/** Wait times in ms before each new try to load an avatar that gave an error. */
export const AVATAR_RETRY_MS = [1500, 5000, 15000];

/**
 * The URL of try number `n` (0 is the first try). The query part changes, so the
 * browser asks again. The bridge ignores the query part.
 */
export function avatarTry(src: string, n: number): string {
  return n === 0 ? src : `${src}?try=${n}`;
}
