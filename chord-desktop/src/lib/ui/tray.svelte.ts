// The attachment tray: the files that wait above the composer, one list for each chat.
// A paste, a drop or a file pick adds an item. Nothing uploads until the user sends.
import {
  MAX_TRAY_ITEMS,
  admit,
  fileItem,
  isBusy,
  pathItem,
  previewKind,
  runUploads,
  sendable,
  type TrayItem
} from './traydata';

class Tray {
  private all = $state<Record<string, TrayItem[]>>({});

  items(jid: string): TrayItem[] {
    return this.all[jid] ?? [];
  }

  /** Add files. Returns the problems, one text for each file that did not join. */
  addFiles(jid: string, files: File[]): string[] {
    if (!jid) return [];
    const room = MAX_TRAY_ITEMS - this.items(jid).length;
    const { accepted, problems } = admit(files, Math.max(0, room));
    if (!accepted.length) return problems;
    const added = accepted.map((f) => fileItem(f, previewKind(f.type) ? URL.createObjectURL(f) : null));
    this.all[jid] = [...this.items(jid), ...added];
    return problems;
  }

  /** Add a file by the path of a drop event. Only Rust can read it. */
  addPath(jid: string, path: string): string[] {
    if (!jid) return [];
    if (this.items(jid).length >= MAX_TRAY_ITEMS) {
      return [`A message can carry ${MAX_TRAY_ITEMS} files at most.`];
    }
    this.all[jid] = [...this.items(jid), pathItem(path)];
    return [];
  }

  /** Remove one item, and free its thumbnail. */
  remove(jid: string, id: string) {
    const list = this.items(jid);
    const item = list.find((i) => i.id === id);
    if (!item || item.status === 'uploading') return;
    if (item.preview) URL.revokeObjectURL(item.preview);
    this.set(
      jid,
      list.filter((i) => i.id !== id)
    );
  }

  /** Remove every item that does not upload now. */
  clear(jid: string) {
    const keep = this.items(jid).filter((i) => i.status === 'uploading');
    for (const i of this.items(jid)) if (i.status !== 'uploading' && i.preview) URL.revokeObjectURL(i.preview);
    this.set(jid, keep);
  }

  /** True when a send has something to upload. */
  hasSendable(jid: string): boolean {
    return sendable(this.items(jid)).length > 0;
  }

  busy(jid: string): boolean {
    return isBusy(this.items(jid));
  }

  /**
   * Upload the items of a chat, in order. An item that goes leaves the tray. An item that
   * fails stays with its error, so the user can try again. Returns the number that failed.
   */
  async sendAll(jid: string, upload: (item: TrayItem) => Promise<string | null>): Promise<number> {
    const take = sendable(this.items(jid));
    if (!take.length) return 0;
    const patch = (id: string, change: Partial<TrayItem>) => {
      const item = this.items(jid).find((i) => i.id === id);
      if (item) Object.assign(item, change);
    };
    return runUploads(take, {
      upload,
      start: (id) => patch(id, { status: 'uploading', error: null }),
      fail: (id, error) => patch(id, { status: 'failed', error }),
      done: (id) => {
        const item = this.items(jid).find((i) => i.id === id);
        if (item?.preview) URL.revokeObjectURL(item.preview);
        this.set(
          jid,
          this.items(jid).filter((i) => i.id !== id)
        );
      }
    });
  }

  private set(jid: string, list: TrayItem[]) {
    if (list.length) this.all[jid] = list;
    else delete this.all[jid];
  }
}

export const tray = new Tray();
