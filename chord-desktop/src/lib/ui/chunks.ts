// The timeline keeps only the rows near the view on the page. The rows split into chunks.
// A chunk far from the view has no rows: it keeps an empty box with the height that it had.
// This file holds the pure logic: how rows join a chunk, and which chunks stay on the page.

export interface Chunk<R> {
  id: number;
  rows: R[];
  /** How many rows count as messages. The "older messages" bar counts them. */
  weight: number;
}

export interface ChunkState {
  /** The chunk of each row key. A row keeps its chunk while it stays in the list. */
  of: Map<string, number>;
  next: number;
}

export function newChunkState(): ChunkState {
  return { of: new Map(), next: 1 };
}

/**
 * Split `rows` into chunks of about `size` rows. A row that was in a chunk before stays in
 * it. So older rows that arrive above the list make new chunks, and the old chunks keep their
 * id, their box and their height. A new row joins the chunk above it until that is full.
 * A chunk never ends with a row that `sticky` names (a date line): the line stays with the
 * row below it. A chunk object stays the same when its rows stay the same.
 */
export function splitRows<R extends { key: string }>(
  rows: R[],
  state: ChunkState,
  prev: Chunk<R>[],
  size: number,
  weigh: (row: R) => number,
  sticky: (row: R) => boolean
): Chunk<R>[] {
  const out: Chunk<R>[] = [];
  const closed = new Set<number>();
  let cur: Chunk<R> | null = null as Chunk<R> | null;
  const start = (id: number, row: R): Chunk<R> => {
    if (cur) closed.add(cur.id);
    const made: Chunk<R> = { id, rows: [row], weight: 0 };
    out.push(made);
    return made;
  };
  for (const row of rows) {
    let id = state.of.get(row.key);
    if (id !== undefined && closed.has(id)) id = undefined;
    if (cur && id === cur.id) {
      cur.rows.push(row);
    } else if (id !== undefined) {
      cur = start(id, row);
    } else if (cur && (cur.rows.length < size || sticky(cur.rows[cur.rows.length - 1]))) {
      cur.rows.push(row);
    } else {
      cur = start(state.next++, row);
    }
  }
  const of = new Map<string, number>();
  const old = new Map(prev.map((c) => [c.id, c]));
  for (let i = 0; i < out.length; i++) {
    const c = out[i];
    let weight = 0;
    for (const r of c.rows) {
      of.set(r.key, c.id);
      weight += weigh(r);
    }
    c.weight = weight;
    const before = old.get(c.id);
    if (before && before.rows.length === c.rows.length && before.rows.every((r, j) => r === c.rows[j])) {
      out[i] = before;
    }
  }
  state.of = of;
  return out;
}

export interface Span {
  id: number;
  top: number;
  height: number;
}

/**
 * Choose the chunks that stay on the page. A chunk within `mount` pixels of the view comes
 * onto the page. A chunk that is on the page already stays until it is more than `keep`
 * pixels away, so a small scroll back and forth does not build rows again and again.
 * A pinned chunk always stays. Returns `current` when nothing changes.
 */
export function chooseMounted(
  spans: Span[],
  viewTop: number,
  viewBottom: number,
  mount: number,
  keep: number,
  current: ReadonlySet<number>,
  pinned: ReadonlySet<number>
): ReadonlySet<number> {
  const next = new Set<number>();
  for (const s of spans) {
    const bottom = s.top + s.height;
    const near = (m: number) => bottom >= viewTop - m && s.top <= viewBottom + m;
    if (pinned.has(s.id) || near(mount) || (current.has(s.id) && near(keep))) next.add(s.id);
  }
  if (next.size === current.size) {
    let same = true;
    for (const id of next) if (!current.has(id)) same = false;
    if (same) return current;
  }
  return next;
}
