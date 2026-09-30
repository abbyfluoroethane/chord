import { describe, expect, it } from 'vitest';
import { findMention, insertMention, suggestNicks } from './mentions';

describe('findMention', () => {
  it('finds an @ at the start or after a space', () => {
    expect(findMention('@al', 3)).toEqual({ start: 0, query: 'al' });
    expect(findMention('hi @', 4)).toEqual({ start: 3, query: '' });
    expect(findMention('hi\n@bo', 6)).toEqual({ start: 3, query: 'bo' });
  });
  it('ignores an @ inside a word, such as an address', () => {
    expect(findMention('mail me@exa', 11)).toBeNull();
    expect(findMention('hi @al and', 10)).toBeNull();
  });
  it('reads up to the caret only', () => {
    expect(findMention('hi @alice', 6)).toEqual({ start: 3, query: 'al' });
  });
});

describe('suggestNicks', () => {
  const nicks = ['Bob', 'alice', 'Albert', 'carol', 'Rob', 'alice'];
  it('lists names that start with the query first', () => {
    expect(suggestNicks(nicks, 'al')).toEqual(['Albert', 'alice']);
    expect(suggestNicks(nicks, 'ob')).toEqual(['Bob', 'Rob']);
    expect(suggestNicks(nicks, 'b')).toEqual(['Bob', 'Albert', 'Rob']);
  });
  it('lists all names for an empty query, up to the limit', () => {
    expect(suggestNicks(nicks, '', 3)).toHaveLength(3);
  });
  it('finds nothing for an unknown name', () => {
    expect(suggestNicks(nicks, 'zed')).toEqual([]);
  });
});

describe('insertMention', () => {
  it('replaces the query and puts the caret after the space', () => {
    const text = 'hi @al, how';
    const match = findMention(text, 6)!;
    expect(insertMention(text, 6, match, 'alice')).toEqual({
      text: 'hi @alice , how',
      caret: 10
    });
  });
});
