// Pure helpers for the GIF panel: the two column layout and the merge of result pages.
import type { Gif } from '$lib/chord';

/** Add a page of results to the list. A GIF that is in the list already stays out. */
export function mergePage(items: Gif[], page: Gif[]): Gif[] {
  const seen = new Set(items.map((g) => g.slug));
  const fresh: Gif[] = [];
  for (const g of page) {
    if (seen.has(g.slug)) continue;
    seen.add(g.slug);
    fresh.push(g);
  }
  return fresh.length === 0 ? items : [...items, ...fresh];
}

/** Two columns of about the same height: each GIF goes to the shorter one. */
export function splitColumns(items: Gif[]): Gif[][] {
  const cols: Gif[][] = [[], []];
  const heights = [0, 0];
  for (const g of items) {
    const h = g.preview.width > 0 ? g.preview.height / g.preview.width : 1;
    const i = heights[0] <= heights[1] ? 0 : 1;
    cols[i].push(g);
    heights[i] += h;
  }
  return cols;
}
