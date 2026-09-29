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
