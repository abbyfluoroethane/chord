// Rail layout: spaces in order, plus local folders.
// Folders are a UI setting. The core has no support for them. The store
// exposes load and save hooks so the bridge can keep the layout in
// get_settings and set_settings. Until then localStorage does the job.

export interface RailCircle {
  kind: 'circle';
  id: string;
}
export interface RailFolder {
  kind: 'folder';
  id: string;
  name: string | null;
  open: boolean;
  circles: string[];
}
export type RailEntry = RailCircle | RailFolder;
export type DropZone = 'before' | 'into' | 'after';

export interface RailPersistence {
  load(): RailEntry[] | null | Promise<RailEntry[] | null>;
  save(layout: RailEntry[]): void | Promise<void>;
}

const KEY = 'chord.rail';

export const localStoragePersistence: RailPersistence = {
  load() {
    try {
      const raw = localStorage.getItem(KEY);
      return raw ? (JSON.parse(raw) as RailEntry[]) : null;
    } catch {
      return null;
    }
  },
  save(layout) {
    try {
      localStorage.setItem(KEY, JSON.stringify(layout));
    } catch {
      /* ignore */
    }
  }
};

export class RailStore {
  layout = $state<RailEntry[]>([]);
  private persistence: RailPersistence;
  /** `init` finished. Before that `sync` must not write, or it would erase the saved layout. */
  private ready = false;

  constructor(persistence: RailPersistence = localStoragePersistence) {
    this.persistence = persistence;
  }

  /** Swap the persistence hooks, for example to the bridge. */
  use(persistence: RailPersistence) {
    this.persistence = persistence;
  }

  /** Load the saved layout and fit it to the spaces that exist now. */
  async init(circleIds: string[]) {
    const saved = (await this.persistence.load()) ?? [];
    this.layout = this.reconcile(saved, circleIds);
    this.ready = true;
  }

  /** Drop spaces that are gone. Append new spaces at the end. */
  reconcile(saved: RailEntry[], circleIds: string[]): RailEntry[] {
    const known = new Set(circleIds);
    const seen = new Set<string>();
    const out: RailEntry[] = [];
    for (const e of saved) {
      if (e.kind === 'circle') {
        if (known.has(e.id) && !seen.has(e.id)) {
          seen.add(e.id);
          out.push({ kind: 'circle', id: e.id });
        }
      } else {
        const inside = e.circles.filter((c) => known.has(c) && !seen.has(c));
        inside.forEach((c) => seen.add(c));
        if (inside.length) out.push({ ...e, circles: inside });
      }
    }
    for (const id of circleIds) if (!seen.has(id)) out.push({ kind: 'circle', id });
    return out;
  }

  /** Call when spaces are added or removed. */
  sync(circleIds: string[]) {
    if (!this.ready) return;
    const next = this.reconcile($state.snapshot(this.layout) as RailEntry[], circleIds);
    if (JSON.stringify(next) !== JSON.stringify($state.snapshot(this.layout))) {
      this.layout = next;
      this.persist();
    }
  }

  private persist() {
    void this.persistence.save($state.snapshot(this.layout) as RailEntry[]);
  }

  folderOf(circleId: string): RailFolder | null {
    for (const e of this.layout) if (e.kind === 'folder' && e.circles.includes(circleId)) return e;
    return null;
  }

  toggle(folderId: string) {
    const f = this.layout.find((e) => e.kind === 'folder' && e.id === folderId);
    if (f && f.kind === 'folder') {
      f.open = !f.open;
      this.persist();
    }
  }

  private removeCircle(id: string) {
    const top = this.layout.findIndex((e) => e.kind === 'circle' && e.id === id);
    if (top >= 0) {
      this.layout.splice(top, 1);
      return;
    }
    const fi = this.layout.findIndex((e) => e.kind === 'folder' && e.circles.includes(id));
    if (fi < 0) return;
    const f = this.layout[fi] as RailFolder;
    f.circles = f.circles.filter((c) => c !== id);
    if (!f.circles.length) this.layout.splice(fi, 1);
  }

  /**
   * Move a space or folder next to a target, or put a space into a
   * folder. ids are space ids or folder ids.
   */
  drop(dragId: string, targetId: string, zone: DropZone) {
    if (dragId === targetId) return;
    const dragFolder = this.layout.find((e): e is RailFolder => e.kind === 'folder' && e.id === dragId);

    if (dragFolder) {
      // A folder only reorders at the top level.
      const target = this.topIndexOf(targetId);
      if (target < 0) return;
      const from = this.layout.indexOf(dragFolder);
      this.layout.splice(from, 1);
      let to = this.topIndexOf(targetId);
      if (zone === 'after' || zone === 'into') to += 1;
      this.layout.splice(to, 0, dragFolder);
      this.persist();
      return;
    }

    // Snapshot the target position before removal so a dissolved folder
    // does not shift the wrong way.
    this.removeCircle(dragId);

    const targetFolder = this.layout.find((e): e is RailFolder => e.kind === 'folder' && e.id === targetId);
    if (targetFolder) {
      if (zone === 'into') {
        targetFolder.circles.push(dragId);
      } else {
        const at = this.layout.indexOf(targetFolder) + (zone === 'after' ? 1 : 0);
        this.layout.splice(at, 0, { kind: 'circle', id: dragId });
      }
      this.persist();
      return;
    }

    const inFolder = this.folderOf(targetId);
    if (inFolder) {
      const at = inFolder.circles.indexOf(targetId) + (zone === 'before' ? 0 : 1);
      inFolder.circles.splice(at, 0, dragId);
      this.persist();
      return;
    }

    const ti = this.layout.findIndex((e) => e.kind === 'circle' && e.id === targetId);
    if (ti < 0) {
      this.layout.push({ kind: 'circle', id: dragId });
    } else if (zone === 'into') {
      const folder: RailFolder = {
        kind: 'folder',
        id: `folder-${Date.now().toString(36)}`,
        name: null,
        open: true,
        circles: [targetId, dragId]
      };
      this.layout.splice(ti, 1, folder);
    } else {
      this.layout.splice(ti + (zone === 'after' ? 1 : 0), 0, { kind: 'circle', id: dragId });
    }
    this.persist();
  }

  /** Take a space out of its folder and put it at the end of the rail. */
  release(circleId: string) {
    if (!this.folderOf(circleId)) return;
    this.removeCircle(circleId);
    this.layout.push({ kind: 'circle', id: circleId });
    this.persist();
  }

  private topIndexOf(id: string): number {
    return this.layout.findIndex(
      (e) => (e.kind === 'circle' && e.id === id) || (e.kind === 'folder' && (e.id === id || e.circles.includes(id)))
    );
  }
}

export const rail = new RailStore();
