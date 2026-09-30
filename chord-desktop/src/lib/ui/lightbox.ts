// Full-screen image viewer: PhotoSwipe 5 (MIT). It loads on the first open only.
// It pages through the images of the open channel, in timeline order.
// Motion: open 240ms (--dur-arrive), close 120ms (--dur-fast), the style guide ease-out cubic-bezier(.2,0,0,1),
// and none with reduced motion.

import 'photoswipe/style.css';
import './lightbox.css';

export interface LightboxImage {
  src: string;
  alt: string;
}

const EASE_OUT = 'cubic-bezier(.2, 0, 0, 1)';
const OPEN_MS = 240;
const CLOSE_MS = 120;
/** The longest wait for the full photo to decode before the viewer opens. */
const DECODE_WAIT_MS = 150;
/** A size for an image that is not measured yet. PhotoSwipe fits it to the screen. */
const FALLBACK = { w: 1600, h: 1200 };

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

/** Load PhotoSwipe before the first open, for example when the pointer is over a photo. */
export function preloadLightbox(): void {
  void import('photoswipe');
}

/** Open the viewer on `images[index]`. */
export async function openLightbox(images: LightboxImage[], index: number): Promise<void> {
  if (images.length === 0) return;
  const start = Math.max(0, Math.min(index, images.length - 1));
  // Only the clicked image is measured before the viewer opens. Its thumbnail is on
  // screen, so it has its size already. The others get their size in the background.
  const [{ default: PhotoSwipe }] = await Promise.all([
    import('photoswipe'),
    measure(images[start].src),
    decodeSoon(images[start].src)
  ]);
  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const pswp = new PhotoSwipe({
    // The placeholder is the thumbnail on screen (`msrc`), not an empty box. The file is
    // the same, so the browser has it already.
    dataSource: images.map((i) => ({ src: i.src, msrc: i.src, alt: i.alt })),
    index: start,
    // The see-through colour and the blur are in lightbox.css.
    bgOpacity: 1,
    showHideAnimationType: still ? 'none' : 'fade',
    showAnimationDuration: still ? 0 : OPEN_MS,
    hideAnimationDuration: still ? 0 : CLOSE_MS,
    easing: EASE_OUT,
    closeTitle: 'Close (Esc)',
    zoomTitle: 'Zoom',
    arrowPrevTitle: 'Previous',
    arrowNextTitle: 'Next',
    errorMsg: 'This image did not load.'
  });
  pswp.addFilter('itemData', (item, i) => {
    const size = sizes.get(images[i].src) ?? FALLBACK;
    return { ...item, width: size.w, height: size.h };
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
  // Measure the other images, and redraw each slide when its size arrives.
  images.forEach((image, i) => {
    if (sizes.has(image.src)) return;
    void measure(image.src).then(() => {
      if (!pswp.isDestroying) pswp.refreshSlideContent(i);
    });
  });
}
