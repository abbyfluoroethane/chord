import { describe, expect, it } from 'vitest';
import { highlightDraft, type Run } from './livemarkdown';

const joined = (runs: Run[]) => runs.map((r) => r.text).join('');
const cls = (draft: string, text: string) =>
  highlightDraft(draft).find((r) => r.text === text)?.cls.split(' ').sort().join(' ');

describe('highlightDraft', () => {
  it('keeps every character, in order', () => {
    const drafts = [
      '',
      'plain text',
      '**bold** and *it* and __u__ and ~~s~~ and ||sp||',
      '***both*** `code **not bold**` \\*escaped\\*',
      '> quote\n>>> rest\nof it',
      '# Head\n## Two\n-# small\n- item\n  - nested\n1. one',
      '```js\nconst a = 1;\n```\nafter',
      'unclosed **bold and `code',
      'snake_case_name and _it_',
      '<t:1735689600:R> [text](https://x.example/a) <https://y.example> @amy :tada:',
      '🎉 emoji 👋🏽 stay\n\n\ntrailing\n'
    ];
    for (const d of drafts) expect(joined(highlightDraft(d))).toBe(d);
  });

  it('styles the inline marks and their text', () => {
    expect(cls('a **bold** b', 'bold')).toBe('b');
    expect(cls('a **bold** b', '**')).toBe('b mk');
    expect(cls('*it*', 'it')).toBe('i');
    expect(cls('__u__', 'u')).toBe('u');
    expect(cls('~~s~~', 's')).toBe('s');
    expect(cls('||hidden||', 'hidden')).toBe('sp');
    expect(cls('***x***', 'x')).toBe('b i');
    expect(cls('**a *b* c**', 'b')).toBe('b i');
  });

  it('keeps code literal', () => {
    expect(cls('`**x**`', '**x**')).toBe('code');
    const block = highlightDraft('```\n**x**\n```');
    expect(block.every((r) => r.cls.includes('code'))).toBe(true);
  });

  it('does not style snake_case or unclosed marks', () => {
    expect(highlightDraft('snake_case_name').every((r) => r.cls === '')).toBe(true);
    expect(highlightDraft('a **b').every((r) => r.cls === '')).toBe(true);
  });

  it('marks line starts: quotes, headings, subtext, lists', () => {
    expect(cls('> hi', '> ')).toBe('mk q');
    expect(cls('# Title', 'Title')).toBe('h');
    expect(cls('-# small', 'small')).toBe('sub');
    expect(cls('- item', '- ')).toBe('mk');
    const multi = highlightDraft('>>> a\nb');
    expect(multi.filter((r) => r.text.includes('b'))[0].cls).toContain('q');
  });

  it('finds timestamps, links, mentions and shortcodes', () => {
    expect(cls('<t:1735689600:R>', '<t:1735689600:R>')).toBe('ts');
    expect(cls('see https://x.example/a.', 'https://x.example/a')).toBe('link');
    expect(cls('[text](https://x.example)', 'text')).toBe('link');
    expect(cls('hi @amy', '@amy')).toBe('at');
    expect(cls('ok :tada:', ':tada:')).toBe('sc');
    expect(cls('a\\*b', '\\')).toBe('mk');
  });
});
