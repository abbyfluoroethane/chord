// The test notice (src-tauri/src/notify.rs).

import { invoke } from '@tauri-apps/api/core';

export const sendTestNotice = () => invoke<void>('send_test_notice');
