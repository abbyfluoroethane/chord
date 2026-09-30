import { describe, expect, it } from 'vitest';
import { highlightCode, languageName } from './highlight';

describe('highlightCode', () => {
  it('builds a tree of scopes and text', async () => {
    const tree = await highlightCode('for x in y:\n  print("a")', 'py');
    expect(tree).not.toBeNull();
    const scopes: string[] = [];
    const text: string[] = [];
    const walk = (n: NonNullable<typeof tree>) => {
      if (n.scope) scopes.push(n.scope);
      for (const c of n.children) {
        if (typeof c === 'string') text.push(c);
        else walk(c);
      }
    };
    walk(tree!);
    expect(scopes.some((s) => s.startsWith('keyword'))).toBe(true);
    expect(text.join('')).toBe('for x in y:\n  print("a")');
  });
  it('returns null for an unknown language', async () => {
    expect(await highlightCode('x', 'nonsense')).toBeNull();
    expect(languageName('TS')).toBe('typescript');
  });
});
