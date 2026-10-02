// Jump to a message that can be older than the loaded ones. The timeline loads older
// pages until the message is in the page, or the start of the chat, or a page limit.

import { app } from './app.svelte';
import { live } from './bridge';
import { jumpToMessage } from './search';
import { ui } from './ui.svelte';

/** Set while a jump loads older pages. The list does not load pages on its own then. */
export const jumping = { busy: false };

/** The most pages of older messages that one jump loads. */
const MAX_PAGES = 20;
const PAGE = 50;

/** Wait until the count of loaded messages changes, or `ms` go by. */
function nextPage(before: number, ms = 3000): Promise<boolean> {
  return new Promise((resolve) => {
    const start = performance.now();
    const check = () => {
      if (app.items.length !== before) resolve(true);
      else if (performance.now() - start > ms) resolve(false);
      else requestAnimationFrame(check);
    };
    check();
  });
}

/**
 * Scroll to the message `id` and flash it. Older pages load first when the message is
 * not in the page. Returns false and tells the user when Chord cannot find it.
 */
export async function jumpTo(id: string): Promise<boolean> {
  if (jumpToMessage(id)) return true;
  if (jumping.busy) return false;
  if (!live) return notFound();
  jumping.busy = true;
  const chat = app.selectedJid;
  try {
    for (let i = 0; i < MAX_PAGES; i++) {
      const before = app.items.length;
      await app.paginateBack(PAGE);
      const grew = await nextPage(before);
      if (app.selectedJid !== chat) return false;
      // Let Svelte put the new rows on the page.
      await new Promise((r) => requestAnimationFrame(r));
      if (jumpToMessage(id)) return true;
      if (!grew) break;
    }
  } finally {
    jumping.busy = false;
  }
  return notFound();
}

function notFound(): false {
  ui.say('Chord could not find that message. The server does not have it.');
  return false;
}
