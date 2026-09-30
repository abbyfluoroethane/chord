// Full-window media viewer: PhotoSwipe 5 (MIT). It loads on the first open only.
// It pages through the photos and videos of the open channel, in timeline order. A video
// slide holds the Chord media player (media/MediaPlayer.svelte).
// Motion: open 240ms (--dur-arrive), close 120ms (--dur-fast), the style guide ease-out
// cubic-bezier(.2,0,0,1), and none with reduced motion.

import { mount, unmount } from 'svelte';
import 'photoswipe/style.css';
import './lightbox.css';
import MediaPlayer from './media/MediaPlayer.svelte';

export interface LightboxItem {
  kind: 'image' | 'video';
  src: string;
  alt: string;
  /** Video: start at this time, in seconds. */
  startAt?: number;
  /** Video: play at once. */
  playing?: boolean;
  /** Video: the viewer closed at this time. The chat player continues from there. */
  onclose?: (time: number) => void;
}

const EASE_OUT = 'cubic-bezier(.2, 0, 0, 1)';
const OPEN_MS = 240;
const CLOSE_MS = 120;
/** The longest wait for the full photo to decode before the viewer opens. */
const DECODE_WAIT_MS = 150;
/** A size for an image that is not measured yet. PhotoSwipe fits it to the screen. */
const FALLBACK = { w: 1600, h: 1200 };
/** A size for a video that is not measured yet. */
const VIDEO_FALLBACK = { w: 1280, h: 720 };
/** The longest wait for the size of a video that is not on screen. */
const VIDEO_WAIT_MS = 1000;

// Lucide icons at the style guide 1.5px line, in place of the PhotoSwipe icons.
const svg = (body: string) =>
  `<svg class="pswp__icn" viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const ICONS = {
  close: svg('<path d="M18 6 6 18"/><path d="m6 6 12 12"/>'),
  // The vertical bar goes away when the photo is zoomed in, so "+" becomes "-".
  zoom: svg(
    '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/><path d="M8 11h6"/><path class="pswp__zoom-icn-bar-v" d="M11 8v6"/>'
  ),
  prev: svg('<path d="m15 18-6-6 6-6"/>'),
  next: svg('<path d="m9 18 6-6-6-6"/>')
};

/** Pixel sizes of the images seen so far, by URL. */
const sizes = new Map<string, { w: number; h: number }>();

/** The size of an image that is on screen already, without a new decode. */
function onScreen(src: string): { w: number; h: number } | null {
  for (const img of document.images) {
    if (img.src === src && img.complete && img.naturalWidth && img.naturalHeight) {
      return { w: img.naturalWidth, h: img.naturalHeight };
    }
  }
  return null;
}

/** The pixel size of an image. The browser cache keeps the file after the first load. */
async function measure(src: string): Promise<{ w: number; h: number }> {
  const known = sizes.get(src) ?? onScreen(src);
  if (known) {
    sizes.set(src, known);
    return known;
  }
  const img = new Image();
  img.src = src;
  let size = FALLBACK;
  try {
    await img.decode();
    if (img.naturalWidth && img.naturalHeight) size = { w: img.naturalWidth, h: img.naturalHeight };
  } catch {
    /* keep the fallback */
  }
  sizes.set(src, size);
  return size;
}

/**
 * Decode the full photo, so that PhotoSwipe can show it on its first frame. Without this,
 * the viewer shows its placeholder until the decode ends, and the photo flickers in. It
 * waits `DECODE_WAIT_MS` at most: a slow photo then shows its thumbnail first.
 */
function decodeSoon(src: string): Promise<void> {
  const img = new Image();
  img.src = src;
  const decoded = img.decode().catch(() => undefined);
  return Promise.race([decoded, new Promise<void>((r) => setTimeout(r, DECODE_WAIT_MS))]);
}

/** The pixel size of a video: from a player on screen, else from its metadata. */
async function measureVideo(src: string): Promise<{ w: number; h: number }> {
  const known = sizes.get(src);
  if (known) return known;
  for (const v of document.querySelectorAll('video')) {
    if (v.currentSrc.startsWith(src) && v.videoWidth && v.videoHeight) {
      const size = { w: v.videoWidth, h: v.videoHeight };
      sizes.set(src, size);
      return size;
    }
  }
  const v = document.createElement('video');
  v.preload = 'metadata';
  v.src = src;
  const size = await new Promise<{ w: number; h: number }>((resolve) => {
    const done = () => resolve(v.videoWidth ? { w: v.videoWidth, h: v.videoHeight } : VIDEO_FALLBACK);
    v.addEventListener('loadedmetadata', done, { once: true });
    v.addEventListener('error', done, { once: true });
    setTimeout(done, VIDEO_WAIT_MS);
  });
  v.removeAttribute('src');
  v.load();
  sizes.set(src, size);
  return size;
}

function measureItem(item: LightboxItem) {
  return item.kind === 'video' ? measureVideo(item.src) : measure(item.src);
}

/** Load PhotoSwipe before the first open, for example when the pointer is over a photo. */
export function preloadLightbox(): void {
  void import('photoswipe');
}

/** Open the viewer on `items[index]`. */
export async function openLightbox(items: LightboxItem[], index: number): Promise<void> {
  if (items.length === 0) return;
  const start = Math.max(0, Math.min(index, items.length - 1));
  const first = items[start];
  // Only the clicked item is measured before the viewer opens. Its thumbnail is on
  // screen, so it has its size already. The others get their size in the background.
  const [{ default: PhotoSwipe }] = await Promise.all([
    import('photoswipe'),
    measureItem(first),
    first.kind === 'image' ? decodeSoon(first.src) : Promise.resolve()
  ]);
  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const pswp = new PhotoSwipe({
    // The placeholder is the thumbnail on screen (`msrc`), not an empty box. The file is
    // the same, so the browser has it already.
    dataSource: items.map((i) =>
      i.kind === 'image' ? { src: i.src, msrc: i.src, alt: i.alt } : { type: 'video', src: i.src, alt: i.alt }
    ),
    index: start,
    // The see-through colour and the blur are in lightbox.css.
    bgOpacity: 1,
    showHideAnimationType: still ? 'none' : 'fade',
    showAnimationDuration: still ? 0 : OPEN_MS,
    hideAnimationDuration: still ? 0 : CLOSE_MS,
    easing: EASE_OUT,
    // The scroll wheel zooms, as a trackpad pinch does.
    wheelToZoom: true,
    closeTitle: 'Close (Esc)',
    zoomTitle: 'Zoom',
    arrowPrevTitle: 'Previous',
    arrowNextTitle: 'Next',
    closeSVG: ICONS.close,
    zoomSVG: ICONS.zoom,
    arrowPrevSVG: ICONS.prev,
    arrowNextSVG: ICONS.next,
    errorMsg: 'This file did not load.'
  });
  pswp.addFilter('itemData', (item, i) => {
    const fallback = items[i].kind === 'video' ? VIDEO_FALLBACK : FALLBACK;
    const size = sizes.get(items[i].src) ?? fallback;
    return { ...item, width: size.w, height: size.h };
  });

  // A video slide mounts the Chord player. Drags and taps on it go to the player, not to
  // PhotoSwipe, so that the seek bar works. The arrow keys still page.
  const players = new Map<object, { player: ReturnType<typeof mount>; item: LightboxItem }>();
  const mediaOf = (content: { element?: HTMLElement }) =>
    content.element?.querySelector('video') ?? null;
  pswp.on('contentLoad', (e) => {
    const { content } = e;
    if (content.type !== 'video') return;
    e.preventDefault();
    const element = document.createElement('div');
    element.className = 'pswp__content pswp__video';
    content.element = element;
    const item = items[content.index];
    const isFirst = content.index === start;
    const player = mount(MediaPlayer, {
      target: element,
      props: {
        kind: 'video',
        src: item.src,
        name: item.alt,
        surface: 'viewer',
        autoplay: isFirst && !!item.playing,
        startAt: isFirst ? (item.startAt ?? 0) : 0
      }
    });
    players.set(content, { player, item });
  });
  pswp.on('pointerDown', (e) => {
    if ((e.originalEvent.target as Element | null)?.closest('.pswp__video')) e.preventDefault();
  });
  pswp.on('contentDeactivate', ({ content }) => mediaOf(content)?.pause());
  // On close, each video stops and gives its time back to the chat player.
  pswp.on('close', () => {
    for (const [content, { item }] of players) {
      const media = mediaOf(content);
      media?.pause();
      item.onclose?.(media?.currentTime ?? 0);
    }
  });
  pswp.on('contentDestroy', ({ content }) => {
    const entry = players.get(content);
    if (!entry) return;
    void unmount(entry.player);
    players.delete(content);
  });
  // Blur the app behind the viewer. A backdrop-filter inside the viewer does not work:
  // PhotoSwipe fades its root with opacity, and that stops a backdrop blur in any child.
  const scrim = document.createElement('div');
  scrim.className = 'lightbox-scrim';
  document.body.append(scrim);
  pswp.on('openingAnimationStart', () => scrim.classList.add('shown'));
  pswp.on('close', () => scrim.classList.remove('shown'));
  pswp.on('destroy', () => setTimeout(() => scrim.remove(), CLOSE_MS));
  pswp.init();
  // Measure the other items, and redraw each slide when its size arrives.
  items.forEach((item, i) => {
    if (sizes.has(item.src)) return;
    void measureItem(item).then(() => {
      if (!pswp.isDestroying) pswp.refreshSlideContent(i);
    });
  });
}
