import { describe, expect, it } from 'vitest';
import type { Gif } from '$lib/chord';
import { mergePage, splitColumns } from './gif-layout';

const gif = (slug: string, w = 100, h = 100): Gif => ({
  slug,
  title: slug,
  preview: { url: `https://x/${slug}.gif`, width: w, height: h },
  full: { url: `https://x/${slug}.gif`, width: w, height: h }
});

describe('mergePage', () => {
  it('appends new GIFs and drops the ones that are in the list', () => {
    const merged = mergePage([gif('a'), gif('b')], [gif('b'), gif('c'), gif('c')]);
    expect(merged.map((g) => g.slug)).toEqual(['a', 'b', 'c']);
  });
  it('keeps the same list when nothing is new', () => {
    const items = [gif('a')];
    expect(mergePage(items, [gif('a')])).toBe(items);
  });
});

describe('splitColumns', () => {
  it('puts each GIF in the shorter column', () => {
    const [l, r] = splitColumns([gif('a', 100, 300), gif('b'), gif('c'), gif('d')]);
    expect(l.map((g) => g.slug)).toEqual(['a']);
    expect(r.map((g) => g.slug)).toEqual(['b', 'c', 'd']);
  });
  it('treats a GIF with no width as square', () => {
    const [l, r] = splitColumns([gif('a', 0, 0), gif('b', 0, 0)]);
    expect(l).toHaveLength(1);
    expect(r).toHaveLength(1);
  });
  it('does not move a GIF when a page is added', () => {
    const first = splitColumns([gif('a'), gif('b', 100, 50), gif('c')]);
    const more = splitColumns([gif('a'), gif('b', 100, 50), gif('c'), gif('d')]);
    expect(more[0].slice(0, first[0].length)).toEqual(first[0]);
    expect(more[1].slice(0, first[1].length)).toEqual(first[1]);
  });
});
