// Full-screen image viewer: PhotoSwipe 5 (MIT). It loads on the first open only.
// It pages through the images of the open channel, in timeline order.
// Motion follows the style guide: arrivals 240ms, departures 120ms, ease-out
// cubic-bezier(.2,0,0,1), and none with reduced motion.

import 'photoswipe/style.css';
import './lightbox.css';

export interface LightboxImage {
  src: string;
  alt: string;
}

const EASE_OUT = 'cubic-bezier(.2, 0, 0, 1)';
const OPEN_MS = 240;
const CLOSE_MS = 120;
/** A size for an image that is not measured yet. PhotoSwipe fits it to the screen. */
const FALLBACK = { w: 1600, h: 1200 };

/** Pixel sizes of the images seen so far, by URL. */
const sizes = new Map<string, { w: number; h: number }>();

/** The pixel size of an image. The browser cache keeps the file after the first load. */
async function measure(src: string): Promise<{ w: number; h: number }> {
  const known = sizes.get(src);
  if (known) return known;
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

/** Open the viewer on `images[index]`. */
export async function openLightbox(images: LightboxImage[], index: number): Promise<void> {
  if (images.length === 0) return;
  const start = Math.max(0, Math.min(index, images.length - 1));
  // Only the clicked image is measured before the viewer opens. It is on screen, so the
  // decode is quick. The others get their size in the background.
  const [{ default: PhotoSwipe }] = await Promise.all([
    import('photoswipe'),
    measure(images[start].src)
  ]);
  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const pswp = new PhotoSwipe({
    dataSource: images.map((i) => ({ src: i.src, alt: i.alt })),
    index: start,
    bgOpacity: 0.92,
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
  pswp.init();
  // Measure the other images, and redraw each slide when its size arrives.
  images.forEach((image, i) => {
    if (sizes.has(image.src)) return;
    void measure(image.src).then(() => {
      if (!pswp.isDestroying) pswp.refreshSlideContent(i);
    });
  });
}
