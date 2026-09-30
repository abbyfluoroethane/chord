import { describe, expect, it } from 'vitest';
import { MAX_DRAFT_CHARS, MAX_TOTAL_CHARS, parseDrafts, putDraft } from './draftdata';

describe('putDraft', () => {
  it('keeps one draft for each chat', () => {
    let d = putDraft({}, 'a@x', 'hello');
    d = putDraft(d, 'b@x', 'world');
    d = putDraft(d, 'a@x', 'hello again');
    expect(d).toEqual({ 'b@x': 'world', 'a@x': 'hello again' });
  });
  it('removes the draft of a blank text', () => {
    expect(putDraft({ 'a@x': 'hi' }, 'a@x', '  \n')).toEqual({});
  });
  it('cuts a long draft', () => {
    const d = putDraft({}, 'a@x', 'x'.repeat(MAX_DRAFT_CHARS + 50));
    expect(d['a@x']).toHaveLength(MAX_DRAFT_CHARS);
  });
  it('drops the oldest drafts when the total is too big, never the new one', () => {
    let d = {};
    const n = Math.ceil(MAX_TOTAL_CHARS / MAX_DRAFT_CHARS) + 2;
    for (let i = 0; i < n; i++) d = putDraft(d, `c${i}@x`, 'y'.repeat(MAX_DRAFT_CHARS));
    const keys = Object.keys(d);
    expect(keys).not.toContain('c0@x');
    expect(keys).toContain(`c${n - 1}@x`);
    expect(Object.values(d).join('').length).toBeLessThanOrEqual(MAX_TOTAL_CHARS);
  });
});

describe('parseDrafts', () => {
  it('keeps strings and drops the rest', () => {
    expect(parseDrafts({ a: 'hi', b: 3, c: '  ', d: null })).toEqual({ a: 'hi' });
    expect(parseDrafts(undefined)).toEqual({});
    expect(parseDrafts([1])).toEqual({});
  });
});
