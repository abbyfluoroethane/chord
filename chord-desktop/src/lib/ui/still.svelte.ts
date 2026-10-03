// Freeze an animated image on its first frame. The action puts a canvas over the image.
// The canvas works for any host: it draws the image and never reads the pixels back.
import { prefs } from './prefs.svelte';
import type { AnimateGifs } from './prefsdata';

/** Types that can animate. A frozen still image of these types looks the same. */
const MOVING = /^image\/(gif|webp|apng)$/i;

/** True when the mime type or the data URL can hold an animation. */
export function mayMove(type: string | null | undefined): boolean {
  if (!type) return false;
  const m = /^data:([^;,]+)/.exec(type);
  return MOVING.test(m ? m[1] : type);
}

export interface StillOptions {
  /** False for an image that cannot move. The action then does nothing. */
  moving: boolean;
}

export function still(img: HTMLImageElement, options: StillOptions) {
  let opts = options;
  let canvas: HTMLCanvasElement | null = null;
  let mode: AnimateGifs = 'always';
  let hover = false;
  let seen: ResizeObserver | null = null;
  const host = img.parentElement;

  function place() {
    if (!canvas) return;
    const cs = getComputedStyle(img);
    Object.assign(canvas.style, {
      left: `${img.offsetLeft}px`,
      top: `${img.offsetTop}px`,
      width: `${img.offsetWidth}px`,
      height: `${img.offsetHeight}px`,
      borderRadius: cs.borderRadius,
      objectFit: cs.objectFit
    });
  }

  function draw() {
    if (!canvas || !img.naturalWidth) return;
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    canvas.getContext('2d')?.drawImage(img, 0, 0);
    place();
  }

  function sync() {
    const frozen = opts.moving && mode !== 'always' && !(mode === 'hover' && hover);
    if (!frozen) {
      canvas?.remove();
      canvas = null;
      seen?.disconnect();
      seen = null;
      return;
    }
    if (canvas) return;
    canvas = document.createElement('canvas');
    canvas.setAttribute('aria-hidden', 'true');
    Object.assign(canvas.style, { position: 'absolute', pointerEvents: 'none' });
    if (host && getComputedStyle(host).position === 'static') host.style.position = 'relative';
    img.after(canvas);
    seen = new ResizeObserver(place);
    seen.observe(img);
    if (img.complete) draw();
  }

  const enter = () => {
    hover = true;
    sync();
  };
  const leave = () => {
    hover = false;
    sync();
  };
  const loaded = () => draw();
  img.addEventListener('load', loaded);
  host?.addEventListener('pointerenter', enter);
  host?.addEventListener('pointerleave', leave);

  const stop = $effect.root(() => {
    $effect(() => {
      mode = prefs.animateGifs;
      sync();
    });
  });

  return {
    update(o: StillOptions) {
      opts = o;
      sync();
    },
    destroy() {
      stop();
      canvas?.remove();
      seen?.disconnect();
      img.removeEventListener('load', loaded);
      host?.removeEventListener('pointerenter', enter);
      host?.removeEventListener('pointerleave', leave);
    }
  };
}
