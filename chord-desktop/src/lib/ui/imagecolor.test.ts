import { describe, expect, it } from 'vitest';
import { pickColor } from './imagecolor';

const px = (list: [number, number, number, number?][]) =>
  list.flatMap(([r, g, b, a = 255]) => [r, g, b, a]);
const hue = (c: string | null) => Number(c?.match(/^hsl\((\d+)/)?.[1]);

describe('pickColor', () => {
  it('finds the main accent, not the grey background', () => {
    const grey = Array.from({ length: 80 }, () => [120, 120, 120] as [number, number, number]);
    const red = Array.from({ length: 20 }, () => [220, 40, 40] as [number, number, number]);
    const c = pickColor(px([...grey, ...red]));
    expect(hue(c)).toBeLessThan(15);
  });

  it('prefers the larger colour area', () => {
    const blue = Array.from({ length: 60 }, () => [40, 90, 220] as [number, number, number]);
    const green = Array.from({ length: 20 }, () => [40, 200, 60] as [number, number, number]);
    const h = hue(pickColor(px([...blue, ...green])));
    expect(h).toBeGreaterThan(200);
    expect(h).toBeLessThan(240);
  });

  it('averages a black and white image to a low saturation', () => {
    const c = pickColor(px([[250, 250, 250], [10, 10, 10], [128, 128, 128]]));
    expect(c).toMatch(/^hsl\(\d+ ([0-9]|[12][0-9]|30)% /);
  });

  it('keeps the lightness between 22% and 45%', () => {
    const light = pickColor(px([[255, 240, 120], [255, 235, 110]]));
    const l = Number(light?.match(/(\d+)%\)$/)?.[1]);
    expect(l).toBeGreaterThanOrEqual(22);
    expect(l).toBeLessThanOrEqual(45);
  });

  it('skips transparent pixels, and gives null for none', () => {
    expect(pickColor(px([[255, 0, 0, 0]]))).toBeNull();
    expect(pickColor([])).toBeNull();
  });
});
