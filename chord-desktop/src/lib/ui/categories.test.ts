import { describe, expect, it } from 'vitest';
import { groupByCategory } from './categories';

const ch = (id: string, category?: string | null) => ({ id, category });

describe('groupByCategory', () => {
  it('makes one group with no name when no row has a category', () => {
    const g = groupByCategory([ch('a'), ch('b', null)]);
    expect(g).toHaveLength(1);
    expect(g[0].name).toBeNull();
    expect(g[0].items.map((c) => c.id)).toEqual(['a', 'b']);
  });

  it('puts the rows with no category first, then each category in order', () => {
    const g = groupByCategory([ch('a', 'Dev'), ch('b'), ch('c', 'Fun'), ch('d', 'Dev')]);
    expect(g.map((x) => x.name)).toEqual([null, 'Dev', 'Fun']);
    expect(g[1].items.map((c) => c.id)).toEqual(['a', 'd']);
  });

  it('treats a blank category as none', () => {
    expect(groupByCategory([ch('a', '  ')])[0].name).toBeNull();
  });

  it('returns one empty group for an empty list', () => {
    expect(groupByCategory([])).toEqual([{ name: null, items: [] }]);
  });
});
