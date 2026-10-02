import { describe, expect, it } from 'vitest';
import { MAX_PASTE_BYTES } from './filetransfer';
import {
  MAX_TRAY_ITEMS,
  admit,
  baseName,
  fileItem,
  fileProblem,
  isBusy,
  pathItem,
  previewKind,
  runUploads,
  sendable,
  type TrayItem
} from './traydata';

const file = (name: string, size = 10, type = 'image/png') => ({ name, size, type }) as File;

describe('tray limits', () => {
  it('refuses an empty file and a file over the size limit with a clear text', () => {
    expect(fileProblem(file('a.png', 0))).toMatch(/empty/);
    expect(fileProblem(file('big.mov', MAX_PASTE_BYTES + 1))).toMatch(/big\.mov is too big/);
    expect(fileProblem(file('big.mov', MAX_PASTE_BYTES + 1))).toMatch(/25\.0 MB/);
    expect(fileProblem(file('a.png', MAX_PASTE_BYTES))).toBeNull();
  });

  it('keeps the good files and reports the bad ones', () => {
    const r = admit([file('a'), file('b', 0), file('c')], 5);
    expect(r.accepted.map((f) => f.name)).toEqual(['a', 'c']);
    expect(r.problems).toHaveLength(1);
  });

  it('stops at the free places', () => {
    const r = admit([file('a'), file('b'), file('c')], 2);
    expect(r.accepted.map((f) => f.name)).toEqual(['a', 'b']);
    expect(r.problems[0]).toContain(String(MAX_TRAY_ITEMS));
    expect(admit([file('a')], 0).accepted).toEqual([]);
  });
});

describe('tray items', () => {
  it('makes an item with a unique id for a file', () => {
    const a = fileItem(file('a.png', 5), 'blob:x');
    const b = fileItem(file('a.png', 5), null);
    expect(a.id).not.toBe(b.id);
    expect(a).toMatchObject({ name: 'a.png', size: 5, status: 'ready', preview: 'blob:x' });
  });

  it('reads the name of a dropped path', () => {
    expect(baseName('/Users/a/Pictures/cat.png')).toBe('cat.png');
    expect(baseName('C:\\Users\\a\\doc.pdf')).toBe('doc.pdf');
    const item = pathItem('/tmp/x/report.pdf');
    expect(item).toMatchObject({ name: 'report.pdf', size: null, path: '/tmp/x/report.pdf' });
  });

  it('shows a thumbnail for an image or a video only', () => {
    expect(previewKind('image/png')).toBe('image');
    expect(previewKind('video/mp4')).toBe('video');
    expect(previewKind('application/pdf')).toBeNull();
  });
});

describe('send order', () => {
  const make = (...names: string[]): TrayItem[] => names.map((n) => fileItem(file(n), null));

  it('uploads in tray order, one at a time', async () => {
    const items = make('a', 'b', 'c');
    const log: string[] = [];
    let running = 0;
    const failed = await runUploads(items, {
      upload: async (i) => {
        running++;
        expect(running).toBe(1);
        log.push(`up ${i.name}`);
        await Promise.resolve();
        running--;
        return null;
      },
      start: (id) => log.push(`start ${items.find((i) => i.id === id)?.name}`),
      fail: () => log.push('fail'),
      done: (id) => log.push(`done ${items.find((i) => i.id === id)?.name}`)
    });
    expect(failed).toBe(0);
    expect(log).toEqual(['start a', 'up a', 'done a', 'start b', 'up b', 'done b', 'start c', 'up c', 'done c']);
  });

  it('keeps a failed item and goes on with the next one', async () => {
    const items = make('a', 'b');
    const failures: Record<string, string> = {};
    const done: string[] = [];
    const failed = await runUploads(items, {
      upload: async (i) => (i.name === 'a' ? 'The server refused it.' : null),
      start: () => {},
      fail: (id, e) => (failures[id] = e),
      done: (id) => done.push(id)
    });
    expect(failed).toBe(1);
    expect(failures[items[0].id]).toBe('The server refused it.');
    expect(done).toEqual([items[1].id]);
  });

  it('turns a thrown error into a failure', async () => {
    const items = make('a');
    let error = '';
    await runUploads(items, {
      upload: async () => {
        throw new Error('offline');
      },
      start: () => {},
      fail: (_id, e) => (error = e),
      done: () => {}
    });
    expect(error).toBe('offline');
  });

  it('takes the failed items again, and not the ones that upload now', () => {
    const items = make('a', 'b', 'c');
    items[0].status = 'failed';
    items[1].status = 'uploading';
    expect(sendable(items).map((i) => i.name)).toEqual(['a', 'c']);
    expect(isBusy(items)).toBe(true);
    expect(isBusy(make('a'))).toBe(false);
  });
});
