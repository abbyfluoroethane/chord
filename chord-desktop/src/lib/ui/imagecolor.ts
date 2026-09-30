// A banner colour from a profile picture: the main accent of the image, or its average
// when the image has no clear colour (black and white, or very dark or light).

/** The size of the small copy that Chord reads the colours from. */
const SAMPLE = 24;
/** Hue buckets of 30 degrees. */
const BUCKETS = 12;

function toHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === rn) h = (gn - bn) / d + (gn < bn ? 6 : 0);
  else if (max === gn) h = (bn - rn) / d + 2;
  else h = (rn - gn) / d + 4;
  return [h * 60, s, l];
}

/**
 * The banner colour for RGBA pixels. It takes the hue bucket with the most colour
 * weight, and averages the pixels in it. With no colourful pixels, it averages all of
 * them. The lightness stays between 22% and 45%, so the banner is never glaring and
 * the avatar ring stays clear. Returns an `hsl()` string, or null for no pixels.
 */
export function pickColor(pixels: Uint8ClampedArray | number[]): string | null {
  const weight = new Array<number>(BUCKETS).fill(0);
  const sums = Array.from({ length: BUCKETS }, () => [0, 0, 0, 0]);
  const all = [0, 0, 0, 0];
  for (let i = 0; i + 3 < pixels.length; i += 4) {
    const [r, g, b, a] = [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]];
    if (a < 128) continue;
    all[0] += r;
    all[1] += g;
    all[2] += b;
    all[3] += 1;
    const [h, s, l] = toHsl(r, g, b);
    if (s < 0.25 || l < 0.12 || l > 0.9) continue;
    const bucket = Math.floor(h / (360 / BUCKETS)) % BUCKETS;
    // Strong, mid-light colours count most.
    const w = s * (1 - Math.abs(l - 0.5) * 1.6);
    weight[bucket] += w;
    sums[bucket][0] += r * w;
    sums[bucket][1] += g * w;
    sums[bucket][2] += b * w;
    sums[bucket][3] += w;
  }
  if (all[3] === 0) return null;
  const best = weight.indexOf(Math.max(...weight));
  // A clear accent needs some weight: about 4% of the pixels at full colour.
  const useAccent = weight[best] > all[3] * 0.04;
  const [r, g, b] = useAccent
    ? [sums[best][0] / sums[best][3], sums[best][1] / sums[best][3], sums[best][2] / sums[best][3]]
    : [all[0] / all[3], all[1] / all[3], all[2] / all[3]];
  const [h, s, l] = toHsl(r, g, b);
  const light = Math.min(0.45, Math.max(0.22, l));
  const sat = useAccent ? Math.min(0.75, Math.max(0.35, s)) : Math.min(0.3, s);
  return `hsl(${Math.round(h)} ${Math.round(sat * 100)}% ${Math.round(light * 100)}%)`;
}

const cache = new Map<string, Promise<string | null>>();

/**
 * The banner colour for an image URL, or null if the image does not load or the page
 * cannot read its pixels. The answer is kept for each URL.
 */
export function bannerColor(src: string): Promise<string | null> {
  let hit = cache.get(src);
  if (hit) return hit;
  hit = new Promise((resolve) => {
    const img = new Image();
    // The avatar URL is another origin. With CORS, the canvas can read the pixels.
    img.crossOrigin = 'anonymous';
    img.decoding = 'async';
    img.onload = () => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = SAMPLE;
        canvas.height = SAMPLE;
        const ctx = canvas.getContext('2d', { willReadFrequently: true });
        if (!ctx) return resolve(null);
        ctx.drawImage(img, 0, 0, SAMPLE, SAMPLE);
        resolve(pickColor(ctx.getImageData(0, 0, SAMPLE, SAMPLE).data));
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = src;
  });
  cache.set(src, hit);
  return hit;
}
