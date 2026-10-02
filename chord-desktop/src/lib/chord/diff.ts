import type { ListDiff } from './types';

/**
 * Apply one diff to a list. Pure: it returns a new array and leaves `list` alone.
 * A diff with a bad index throws a RangeError, because that means the UI lost a diff.
 */
export function applyDiff<T>(list: readonly T[], diff: ListDiff<T>): T[] {
  switch (diff.type) {
    case 'reset':
      return diff.items.slice();
    case 'insert': {
      if (diff.index < 0 || diff.index > list.length) {
        throw new RangeError(`insert at ${diff.index} in a list of ${list.length}`);
      }
      return [...list.slice(0, diff.index), diff.item, ...list.slice(diff.index)];
    }
    case 'update': {
      if (diff.index < 0 || diff.index >= list.length) {
        throw new RangeError(`update at ${diff.index} in a list of ${list.length}`);
      }
      const next = list.slice();
      next[diff.index] = diff.item;
      return next;
    }
    case 'remove': {
      if (diff.index < 0 || diff.index >= list.length) {
        throw new RangeError(`remove at ${diff.index} in a list of ${list.length}`);
      }
      return [...list.slice(0, diff.index), ...list.slice(diff.index + 1)];
    }
  }
}

/** Apply several diffs in order. */
export function applyDiffs<T>(list: readonly T[], diffs: readonly ListDiff<T>[]): T[] {
  return diffs.reduce<T[]>((acc, diff) => applyDiff(acc, diff), list.slice());
}

/** True when two plain values hold the same data. Handles objects, arrays and primitives. */
export function sameData(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) {
    const other = b as unknown[];
    if (a.length !== other.length) return false;
    for (let i = 0; i < a.length; i++) if (!sameData(a[i], other[i])) return false;
    return true;
  }
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  const keys = Object.keys(x);
  if (keys.length !== Object.keys(y).length) return false;
  for (const k of keys) if (!(k in y) || !sameData(x[k], y[k])) return false;
  return true;
}

/**
 * Give each new row the object of the old row with the same key, when both hold the same
 * data. A keyed list then keeps the identity of the rows that did not change, so the
 * rows do not redraw. Pure: `next` is not changed.
 */
export function reuseRows<T>(prev: readonly T[], next: readonly T[], key: (row: T) => string): T[] {
  if (prev.length === 0) return next.slice();
  const old = new Map<string, T>();
  for (const row of prev) old.set(key(row), row);
  return next.map((row) => {
    const before = old.get(key(row));
    return before !== undefined && sameData(before, row) ? before : row;
  });
}
