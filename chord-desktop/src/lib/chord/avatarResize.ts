// Shrink a new avatar before it goes out (XEP-0084 4 and 5): at most 256 by 256 pixels, as
// PNG or JPEG, and a few tens of KiB. The core refuses more than AVATAR_MAX_BYTES.
// The pure steps are here so that vitest can run them. `resizeAvatar` runs in the page.

export const AVATAR_MAX_SIDE = 256;
// The same limit as MAX_PUBLISH_BYTES in chord-core.
export const AVATAR_MAX_BYTES = 64 * 1024;

export interface Encoded {
  mime: 'image/png' | 'image/jpeg';
  bytes: Uint8Array;
  width: number;
  height: number;
}

// Encode the image at `width` by `height`. `quality` is for JPEG only.
export type Encoder = (
  mime: Encoded['mime'],
  width: number,
  height: number,
  quality: number,
) => Promise<Uint8Array>;

// The size that fits in `max` by `max` with the same ratio. A small image stays as it is.
export function fitWithin(width: number, height: number, max = AVATAR_MAX_SIDE) {
  if (width <= max && height <= max) return { width, height };
  const scale = max / Math.max(width, height);
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

const QUALITIES = [0.9, 0.8, 0.7, 0.6, 0.5, 0.4];

// Find the first encoding that fits in `limit`. PNG first (it keeps transparency). Then
// JPEG from good to poor quality. Then the same again at three quarters of the size.
// Returns null when nothing fits.
export async function encodeUnderLimit(
  encode: Encoder,
  width: number,
  height: number,
  limit = AVATAR_MAX_BYTES,
): Promise<Encoded | null> {
  let w = width;
  let h = height;
  for (let round = 0; round < 4; round++) {
    const png = await encode('image/png', w, h, 1);
    if (png.length <= limit) return { mime: 'image/png', bytes: png, width: w, height: h };
    for (const quality of QUALITIES) {
      const jpeg = await encode('image/jpeg', w, h, quality);
      if (jpeg.length <= limit) {
        return { mime: 'image/jpeg', bytes: jpeg, width: w, height: h };
      }
    }
    w = Math.max(1, Math.round(w * 0.75));
    h = Math.max(1, Math.round(h * 0.75));
  }
  return null;
}

// Decode `file`, shrink it, and encode it under the limit. Throws a message for the user.
export async function resizeAvatar(file: Blob): Promise<Encoded> {
  const bitmap = await createImageBitmap(file);
  try {
    const fit = fitWithin(bitmap.width, bitmap.height);
    const encode: Encoder = async (mime, width, height, quality) => {
      const canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d');
      if (!context) throw new Error('This browser cannot shrink the image.');
      // A JPEG has no transparency. Paint white under it.
      if (mime === 'image/jpeg') {
        context.fillStyle = '#ffffff';
        context.fillRect(0, 0, width, height);
      }
      context.drawImage(bitmap, 0, 0, width, height);
      const blob = await new Promise<Blob | null>((done) =>
        canvas.toBlob(done, mime, quality),
      );
      if (!blob) throw new Error('This browser cannot shrink the image.');
      return new Uint8Array(await blob.arrayBuffer());
    };
    const result = await encodeUnderLimit(encode, fit.width, fit.height);
    if (!result) throw new Error('The image is too detailed to shrink. Pick another image.');
    return result;
  } finally {
    bitmap.close();
  }
}
