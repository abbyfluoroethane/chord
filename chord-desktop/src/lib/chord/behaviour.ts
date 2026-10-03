// The window behaviour: login entry, tray icon, and system idle time (src-tauri/src/behaviour.rs).

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Does the OS start Chord at login? */
export const getAutostart = () => invoke<boolean>('get_autostart');
export const setAutostart = (enabled: boolean) => invoke<void>('set_autostart', { enabled });
/** The unread total, for the dot on the tray icon. */
export const setTrayUnread = (count: number) => invoke<void>('set_tray_unread', { count });
/** Seconds since the last input in the whole session, or null when the OS gives none. */
export const systemIdleSeconds = () => invoke<number | null>('system_idle_seconds');

/** The status that a tray menu item picks. */
export type TrayStatus = 'chat' | 'away' | 'dnd';

/** Call `handler` when the user picks a status in the tray menu. Returns the stop function. */
export async function onTrayStatus(handler: (status: TrayStatus) => void): Promise<() => void> {
  return listen<TrayStatus>('tray-status', (e) => handler(e.payload));
}
