// Full-screen image viewer: PhotoSwipe 5 (MIT). It loads on the first open only.
// It pages through the images of the open channel, in timeline order.

import 'photoswipe/style.css';

export interface LightboxImage {
  src: string;
  alt: string;
}

/** The pixel size of an image. The browser cache keeps the file after the first load. */
async function size(src: string): Promise<{ w: number; h: number }> {
  const img = new Image();
  img.src = src;
  try {
    await img.decode();
    return { w: img.naturalWidth || 1600, h: img.naturalHeight || 1200 };
  } catch {
    return { w: 1600, h: 1200 };
  }
}

/** Open the viewer on `images[index]`. */
export async function openLightbox(images: LightboxImage[], index: number): Promise<void> {
  if (images.length === 0) return;
  const [{ default: PhotoSwipe }, sizes] = await Promise.all([
    import('photoswipe'),
    Promise.all(images.map((i) => size(i.src)))
  ]);
  const pswp = new PhotoSwipe({
    dataSource: images.map((i, n) => ({ src: i.src, alt: i.alt, width: sizes[n].w, height: sizes[n].h })),
    index: Math.max(0, Math.min(index, images.length - 1)),
    bgOpacity: 0.92,
    showHideAnimationType: 'fade',
    closeTitle: 'Close (Esc)',
    zoomTitle: 'Zoom',
    arrowPrevTitle: 'Previous',
    arrowNextTitle: 'Next',
    errorMsg: 'This image did not load.'
  });
  pswp.init();
}
