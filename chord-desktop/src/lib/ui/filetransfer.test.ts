import { describe, expect, it } from 'vitest';
import { MAX_PASTE_BYTES, dropAction, pasteProblem, pastedFiles } from './filetransfer';

describe('dropAction', () => {
  it('shows the hint while a drag is over the chat, and uploads on the drop', () => {
    expect(dropAction('enter', false)).toBe('show');
    expect(dropAction('over', false)).toBe('show');
    expect(dropAction('leave', false)).toBe('hide');
    expect(dropAction('drop', false)).toBe('upload');
  });

  it('takes no file on the contacts page', () => {
    expect(dropAction('enter', true)).toBe('ignore');
    expect(dropAction('over', true)).toBe('ignore');
    expect(dropAction('drop', true)).toBe('hide');
    expect(dropAction('leave', true)).toBe('hide');
  });

  it('ignores an unknown event', () => {
    expect(dropAction('other', false)).toBe('ignore');
  });
});

describe('pasted files', () => {
  it('reads the files of a paste', () => {
    const png = new File([new Uint8Array([1, 2, 3])], 'image.png', { type: 'image/png' });
    expect(pastedFiles({ files: [png] })).toEqual([png]);
  });

  it('gives no file for a text paste', () => {
    expect(pastedFiles({ files: [] })).toEqual([]);
    expect(pastedFiles({})).toEqual([]);
    expect(pastedFiles(null)).toEqual([]);
  });

  it('refuses an empty file and a file over the cap', () => {
    expect(pasteProblem({ size: 0 })).toMatch(/empty/);
    expect(pasteProblem({ size: MAX_PASTE_BYTES + 1 })).toMatch(/25 MB/);
    expect(pasteProblem({ size: 10 })).toBeNull();
    expect(pasteProblem({ size: MAX_PASTE_BYTES })).toBeNull();
  });
});
