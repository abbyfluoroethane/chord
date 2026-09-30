import { describe, expect, it } from 'vitest';
import { failureNote, runBatch } from './batch';

describe('failureNote', () => {
  it('says nothing when all steps passed', () => {
    expect(failureNote(0, 5, 'invite')).toBeNull();
  });
  it('counts the failures', () => {
    expect(failureNote(2, 5, 'invite')).toBe('2 of 5 invites failed.');
    expect(failureNote(1, 1, 'invite')).toBe('1 of 1 invite failed.');
  });
});

describe('runBatch', () => {
  it('runs every step and counts the failures', async () => {
    const seen: number[] = [];
    const r = await runBatch([1, 2, 3, 4], async (n) => {
      seen.push(n);
      if (n % 2 === 0) throw new Error(`no ${n}`);
    });
    expect(seen).toEqual([1, 2, 3, 4]);
    expect(r.failed).toBe(2);
    expect((r.first as Error).message).toBe('no 2');
  });
});
