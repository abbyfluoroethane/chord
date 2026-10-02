import { describe, expect, it } from 'vitest';
import { chooseMounted, newChunkState, splitRows, type Chunk } from './chunks';

type R = { key: string; day?: boolean };
// A row keeps its object, as the list does with its cache.
const made = new Map<string, R>();
const row = (key: string): R => made.get(key) ?? (made.set(key, { key }), made.get(key)!);
const rows = (from: number, to: number): R[] =>
  Array.from({ length: to - from }, (_, i) => row(`m${from + i}`));
const split = (list: R[], st: ReturnType<typeof newChunkState>, prev: Chunk<R>[] = [], size = 4) =>
  splitRows(list, st, prev, size, (r) => (r.day ? 0 : 1), (r) => !!r.day);

describe('splitRows', () => {
  it('fills chunks to the size', () => {
    const c = split(rows(0, 10), newChunkState());
    expect(c.map((x) => x.rows.length)).toEqual([4, 4, 2]);
    expect(c.map((x) => x.weight)).toEqual([4, 4, 2]);
  });

  it('keeps chunk ids when older rows arrive above', () => {
    const st = newChunkState();
    const first = split(rows(10, 18), st);
    const ids = first.map((c) => c.id);
    const next = split([...rows(0, 10), ...rows(10, 18)], st, first);
    expect(next.slice(-2).map((c) => c.id)).toEqual(ids);
    expect(next.slice(-2)).toEqual(first);
    expect(next.slice(-2)[0]).toBe(first[0]);
    expect(next.length).toBe(5);
  });

  it('adds new rows to the last chunk, then starts another', () => {
    const st = newChunkState();
    const a = split(rows(0, 6), st);
    const b = split(rows(0, 7), st, a);
    expect(b.map((c) => c.rows.length)).toEqual([4, 3]);
    expect(b[0]).toBe(a[0]);
    expect(b[1]).not.toBe(a[1]);
    const c = split(rows(0, 9), st, b);
    expect(c.map((x) => x.rows.length)).toEqual([4, 4, 1]);
  });

  it('does not end a chunk with a date line', () => {
    const list: R[] = [...rows(0, 3), { key: 'day', day: true }, ...rows(3, 6)];
    const c = split(list, newChunkState());
    expect(c[0].rows.map((r) => r.key)).toEqual(['m0', 'm1', 'm2', 'day', 'm3']);
    expect(c[0].weight).toBe(4);
  });

  it('puts a row that arrives in the middle with the row above it', () => {
    const st = newChunkState();
    const a = split(rows(0, 8), st);
    const mid: R[] = [...rows(0, 2), row('x'), ...rows(2, 8)];
    const b = split(mid, st, a);
    expect(b[0].rows.map((r) => r.key)).toEqual(['m0', 'm1', 'x', 'm2', 'm3']);
    expect(b.map((c) => c.id)).toEqual(a.map((c) => c.id));
  });

  it('forgets rows that left the list', () => {
    const st = newChunkState();
    split(rows(0, 8), st);
    split(rows(4, 8), st);
    expect(st.of.has('m0')).toBe(false);
    expect(st.of.size).toBe(4);
  });
});

describe('chooseMounted', () => {
  const spans = Array.from({ length: 10 }, (_, i) => ({ id: i, top: i * 100, height: 100 }));
  const none = new Set<number>();

  it('mounts the chunks near the view', () => {
    const set = chooseMounted(spans, 500, 600, 100, 200, none, none);
    expect([...set].sort()).toEqual([3, 4, 5, 6, 7]);
  });

  it('keeps a mounted chunk inside the keep margin', () => {
    const cur = new Set([2, 3, 4, 5, 6, 7]);
    const set = chooseMounted(spans, 500, 600, 100, 200, cur, none);
    expect(set.has(2)).toBe(true);
    expect(set.has(1)).toBe(false);
  });

  it('always keeps a pinned chunk', () => {
    const set = chooseMounted(spans, 0, 100, 0, 0, none, new Set([9]));
    expect(set.has(9)).toBe(true);
  });

  it('returns the same set when nothing changes', () => {
    const cur = chooseMounted(spans, 500, 600, 100, 200, none, none);
    expect(chooseMounted(spans, 500, 600, 100, 200, cur, none)).toBe(cur);
  });
});
