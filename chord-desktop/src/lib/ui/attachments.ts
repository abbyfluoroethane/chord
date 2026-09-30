// Things to do with a file or a link in a message. The context menu and the message use them.
import { plainError } from './adapt';
import { app } from './app.svelte';
import { api, live } from './bridge';
import { openLightbox, type LightboxItem } from './lightbox';
import type { Attachment } from './types';
import { ui } from './ui.svelte';

/** How a video opens in the viewer: where the chat player stopped, and how to go back. */
export interface VideoHandoff {
  startAt: number;
  playing: boolean;
  onclose: (time: number) => void;
}

/**
 * Open the viewer on a photo or a video. It pages through every photo and video of the
 * open channel.
 */
export function viewMedia(file: Attachment, handoff?: VideoHandoff): void {
  const items: LightboxItem[] = app.items
    .map((m) => m.attachment)
    .filter((a): a is Attachment => !!a && /^(image|video)\//.test(a.mime))
    .map((a) => ({
      kind: a.mime.startsWith('video/') ? 'video' : 'image',
      src: a.url,
      alt: a.name,
      ...(a.url === file.url ? handoff : {})
    }));
  const index = items.findIndex((i) => i.src === file.url);
  void openLightbox(items, index);
}

/** Open the viewer on a photo. */
export function viewImage(file: Attachment): void {
  viewMedia(file);
}

/**
 * Save an image to a place that the user picks. Only inside the app: the save dialog gives
 * the path, and the bridge downloads the file there. The download refuses private
 * addresses and files over 50 MB (see `save_image` in link_preview.rs).
 */
export async function saveImage(file: Attachment): Promise<void> {
  if (!live) return;
  try {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({ title: 'Save image', defaultPath: file.name || 'image' });
    if (!path) return;
    await (await api()).saveImage(file.url, path);
    ui.say('Image saved.');
  } catch (e) {
    ui.say(plainError(e));
  }
}

/** Open a link in the system browser. */
export async function openLink(url: string): Promise<void> {
  if (!/^(https?|mailto):/i.test(url)) return;
  if (!live) {
    window.open(url, '_blank', 'noopener,noreferrer');
    return;
  }
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch (e) {
    ui.say(plainError(e));
  }
}
