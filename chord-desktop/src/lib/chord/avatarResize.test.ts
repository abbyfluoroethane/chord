import { describe, expect, it } from 'vitest';
import { AVATAR_MAX_BYTES, encodeUnderLimit, fitWithin, type Encoder } from './avatarResize';

describe('fitWithin', () => {
  it('keeps a small image as it is', () => {
    expect(fitWithin(100, 80)).toEqual({ width: 100, height: 80 });
    expect(fitWithin(256, 256)).toEqual({ width: 256, height: 256 });
  });

  it('scales a big image to at most 256 on the long side', () => {
    expect(fitWithin(1024, 1024)).toEqual({ width: 256, height: 256 });
    expect(fitWithin(4000, 2000)).toEqual({ width: 256, height: 128 });
    expect(fitWithin(300, 1200)).toEqual({ width: 64, height: 256 });
  });

  it('never gives a side below 1 pixel', () => {
    expect(fitWithin(100000, 10)).toEqual({ width: 256, height: 1 });
  });

  it('takes another maximum', () => {
    expect(fitWithin(1000, 500, 100)).toEqual({ width: 100, height: 50 });
  });
});

// An encoder with a size that follows a rule, and a log of its calls.
function fake(size: (mime: string, w: number, h: number, q: number) => number) {
  const calls: string[] = [];
  const encode: Encoder = async (mime, w, h, q) => {
    calls.push(`${mime}@${w}x${h}${mime === 'image/jpeg' ? `q${q}` : ''}`);
    return new Uint8Array(size(mime, w, h, q));
  };
  return { encode, calls };
}

describe('encodeUnderLimit', () => {
  it('takes the PNG when it fits', async () => {
    const { encode, calls } = fake(() => 1000);
    const out = await encodeUnderLimit(encode, 200, 100);
    expect(out?.mime).toBe('image/png');
    expect(out).toMatchObject({ width: 200, height: 100 });
    expect(calls).toEqual(['image/png@200x100']);
  });

  it('falls back to JPEG, from good quality down', async () => {
    const { encode, calls } = fake((mime, _w, _h, q) =>
      mime === 'image/png' ? AVATAR_MAX_BYTES + 1 : q > 0.75 ? AVATAR_MAX_BYTES + 1 : 500,
    );
    const out = await encodeUnderLimit(encode, 256, 256);
    expect(out?.mime).toBe('image/jpeg');
    expect(calls).toEqual(['image/png@256x256', 'image/jpeg@256x256q0.9', 'image/jpeg@256x256q0.8', 'image/jpeg@256x256q0.7']);
  });

  it('shrinks the size when no quality fits', async () => {
    const { encode } = fake((_mime, w) => (w > 200 ? AVATAR_MAX_BYTES + 1 : 100));
    const out = await encodeUnderLimit(encode, 256, 128);
    expect(out).toMatchObject({ mime: 'image/png', width: 192, height: 96 });
  });

  it('gives null when nothing fits', async () => {
    const { encode } = fake(() => AVATAR_MAX_BYTES + 1);
    expect(await encodeUnderLimit(encode, 256, 256)).toBeNull();
  });

  it('accepts a result of exactly the limit', async () => {
    const { encode } = fake(() => AVATAR_MAX_BYTES);
    expect((await encodeUnderLimit(encode, 10, 10))?.bytes.length).toBe(AVATAR_MAX_BYTES);
  });
});
