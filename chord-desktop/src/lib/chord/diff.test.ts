import { describe, expect, it } from 'vitest';
import { applyDiff, applyDiffs, reuseRows, sameData } from './diff';
import type { ListDiff } from './types';

const reset = (items: string[]): ListDiff<string> => ({ type: 'reset', items });

describe('applyDiff', () => {
  it('resets the list', () => {
    expect(applyDiff(['a'], reset(['x', 'y']))).toEqual(['x', 'y']);
  });

  it('inserts at the start, the middle and the end', () => {
    const list = ['a', 'c'];
    expect(applyDiff(list, { type: 'insert', index: 0, item: 'z' })).toEqual(['z', 'a', 'c']);
    expect(applyDiff(list, { type: 'insert', index: 1, item: 'b' })).toEqual(['a', 'b', 'c']);
    expect(applyDiff(list, { type: 'insert', index: 2, item: 'd' })).toEqual(['a', 'c', 'd']);
  });

  it('updates and removes', () => {
    const list = ['a', 'b', 'c'];
    expect(applyDiff(list, { type: 'update', index: 1, item: 'B' })).toEqual(['a', 'B', 'c']);
    expect(applyDiff(list, { type: 'remove', index: 0 })).toEqual(['b', 'c']);
  });

  it('does not change its input', () => {
    const list = ['a', 'b'];
    applyDiff(list, { type: 'remove', index: 0 });
    applyDiff(list, { type: 'insert', index: 0, item: 'z' });
    applyDiff(list, { type: 'update', index: 0, item: 'z' });
    expect(list).toEqual(['a', 'b']);
  });

  it('throws on a bad index', () => {
    expect(() => applyDiff(['a'], { type: 'insert', index: 2, item: 'x' })).toThrow(RangeError);
    expect(() => applyDiff(['a'], { type: 'update', index: 1, item: 'x' })).toThrow(RangeError);
    expect(() => applyDiff([], { type: 'remove', index: 0 })).toThrow(RangeError);
    expect(() => applyDiff(['a'], { type: 'remove', index: -1 })).toThrow(RangeError);
  });

  it('applies a batch in order, like the Rust diff function', () => {
    // A move of "a" behind "b": remove at 0, insert at 1.
    const diffs: ListDiff<string>[] = [
      { type: 'remove', index: 0 },
      { type: 'insert', index: 1, item: 'a' },
    ];
    expect(applyDiffs(['a', 'b'], diffs)).toEqual(['b', 'a']);
  });
});

describe('sameData', () => {
  it('compares nested data by value', () => {
    expect(sameData({ a: 1, b: [{ c: 'x' }] }, { a: 1, b: [{ c: 'x' }] })).toBe(true);
    expect(sameData({ a: 1, b: [{ c: 'x' }] }, { a: 1, b: [{ c: 'y' }] })).toBe(false);
    expect(sameData({ a: 1 }, { a: 1, b: 2 })).toBe(false);
    expect(sameData([1, 2], [1, 2, 3])).toBe(false);
    expect(sameData(null, {})).toBe(false);
    expect(sameData([], {})).toBe(false);
  });
});

describe('reuseRows', () => {
  const key = (r: { id: string; n?: number[] }) => r.id;

  it('keeps the old object of a row that did not change', () => {
    const a = { id: 'a', n: [1] };
    const b = { id: 'b', n: [2] };
    const next = reuseRows([a, b], [{ id: 'a', n: [1] }, { id: 'b', n: [3] }], key);
    expect(next[0]).toBe(a);
    expect(next[1]).not.toBe(b);
    expect(next[1].n).toEqual([3]);
  });

  it('takes the new rows when the old list is empty', () => {
    const fresh = [{ id: 'a' }];
    const next = reuseRows([], fresh, key);
    expect(next).toEqual(fresh);
    expect(next).not.toBe(fresh);
  });

  it('does not change its input', () => {
    const fresh = [{ id: 'a' }];
    reuseRows([{ id: 'a' }], fresh, key);
    expect(fresh).toEqual([{ id: 'a' }]);
  });
});
