// The app updater (src-tauri/src/updates.rs). See docs/updates.md.

import { Channel, invoke } from '@tauri-apps/api/core';
import type { UpdateChannel, UpdateInfo, UpdateProgress } from './types';

/** Look for an update in a channel. Null when there is none or the build is a dev build. */
export const updateCheck = (channel: UpdateChannel) =>
  invoke<UpdateInfo | null>('update_check', { channel });

/**
 * Download and install the update from the last check. `onProgress` gets the bytes so far.
 * On Windows the installer closes the app. Elsewhere call `updateRestart` after it.
 */
export function updateInstall(onProgress: (p: UpdateProgress) => void): Promise<void> {
  const channel = new Channel<UpdateProgress>();
  channel.onmessage = onProgress;
  return invoke<void>('update_install', { onProgress: channel });
}

/** Start the app again, so the installed update runs. */
export const updateRestart = () => invoke<void>('update_restart');
