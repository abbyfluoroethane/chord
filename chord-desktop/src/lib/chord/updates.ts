// The app updater (src-tauri/src/updates.rs, src-tauri/src/flatpak.rs). See docs/updates.md.

import { Channel, invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { UpdateChannel, UpdateInfo, UpdateProgress } from './types';

/**
 * Look for an update in a channel. Null when there is none or the install has no updater.
 * In a Flatpak the installed branch is the channel, and the Flatpak portal answers.
 */
export const updateCheck = (channel: UpdateChannel) =>
  invoke<UpdateInfo | null>('update_check', { channel });

/**
 * Download and install the update from the last check. `onProgress` gets the bytes so far.
 * In a Flatpak it gets the done part of 100 per step, so the total is always there.
 * On Windows the installer closes the app. Elsewhere call `updateRestart` after it.
 */
export function updateInstall(onProgress: (p: UpdateProgress) => void): Promise<void> {
  const channel = new Channel<UpdateProgress>();
  channel.onmessage = onProgress;
  return invoke<void>('update_install', { onProgress: channel });
}

/** Start the app again, so the installed update runs. In a Flatpak it starts the new version. */
export const updateRestart = () => invoke<void>('update_restart');

/** Call `handler` when the Flatpak portal finds an update. Returns the stop function. */
export async function onUpdateAvailable(handler: (info: UpdateInfo) => void): Promise<() => void> {
  return listen<UpdateInfo>('update-available', (e) => handler(e.payload));
}
