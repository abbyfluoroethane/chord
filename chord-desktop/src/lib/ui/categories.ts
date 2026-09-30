// Channel categories of a space: the rows that share a category sit under one header.
// Plain functions, so that Vitest can test them.

export interface CategoryGroup<T> {
  /** The category name. Null for the rows that have none: they come first, with no header. */
  name: string | null;
  items: T[];
}

/**
 * Group channels by category. The groups keep the order in which a category first appears,
 * and so do the rows inside. The rows with no category form one group at the start.
 * With no category at all there is one group with no name, and the list looks as before.
 */
export function groupByCategory<T extends { category?: string | null }>(
  channels: T[]
): CategoryGroup<T>[] {
  const plain: T[] = [];
  const named = new Map<string, T[]>();
  for (const c of channels) {
    const name = c.category?.trim();
    if (!name) plain.push(c);
    else named.set(name, [...(named.get(name) ?? []), c]);
  }
  const out: CategoryGroup<T>[] = [];
  if (plain.length || named.size === 0) out.push({ name: null, items: plain });
  for (const [name, items] of named) out.push({ name, items });
  return out;
}
