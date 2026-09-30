// The unread count for the window title and the dock badge (src-tauri/src/badge.rs).

import { invoke } from '@tauri-apps/api/core';

export const setUnreadCount = (count: number) => invoke<void>('set_unread_count', { count });
