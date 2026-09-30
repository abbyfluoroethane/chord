import { describe, expect, it } from 'vitest';
import chordDark from './themes/chord-dark.css?raw';
import chordLight from './themes/chord-light.css?raw';
import mocha from './themes/catppuccin-mocha.css?raw';
import latte from './themes/catppuccin-latte.css?raw';
import { sanitizeThemeCss } from './themesafe';

const clean = (css: string) => sanitizeThemeCss(css).css;

describe('sanitizeThemeCss: what stays', () => {
  it('leaves the built-in themes as they are', () => {
    for (const css of [chordDark, chordLight, mocha, latte]) {
      const r = sanitizeThemeCss(css);
      expect(r.blocked).toEqual([]);
      expect(r.css).toBe(css);
    }
  });

  it('keeps tokens, accent rules, selectors, strings and comments', () => {
    const css = `/** @name X\n * @accent blue Blue */
:root { --brand: #89b4fa; --x: "url(https://no.example)"; }
:root[data-accent="blue"] { --brand: #89b4fa; }
.a\\:b::after { content: "\\201C"; }
a[href^="https://bank.example"] { color: red; } /* url(https://c.example) */`;
    const r = sanitizeThemeCss(css);
    expect(r.css).toBe(css);
    expect(r.blocked).toEqual([]);
  });

  it('keeps a data URL and a fragment URL', () => {
    const css = `a { background: url(data:image/png;base64,AAAA); mask: url("data:image/svg+xml;utf8,<svg/>"); fill: url(#g); }`;
    expect(clean(css)).toBe(css);
  });
});

describe('sanitizeThemeCss: what goes', () => {
  it('replaces a remote url() with an empty one', () => {
    for (const u of [
      'url(https://evil.example/?x)',
      'url("https://evil.example/?x")',
      "url('//evil.example/x')",
      'url( http://evil.example/x )',
      'url(chord-avatar://localhost/a)',
      'url(/local.png)',
      'URL(https://evil.example/x)'
    ]) {
      const r = sanitizeThemeCss(`a[href^="https://bank"] { background: ${u}; }`);
      expect(r.css, u).toBe('a[href^="https://bank"] { background: url("data:,"); }');
      expect(r.blocked.length, u).toBe(1);
    }
  });

  it('sees through escapes in the name and the value', () => {
    for (const css of [
      'a { background: u\\72l(https://evil.example/x); }',
      'a { background: \\75rl(https://evil.example/x); }',
      'a { background: url(h\\74tps://evil.example/x); }',
      'a { background: url("ht\\74 tps://evil.example/x"); }'
    ]) {
      const r = sanitizeThemeCss(css);
      expect(r.css, css).toBe('a { background: url("data:,"); }');
    }
  });

  it('finds a url() in a nested function and in a list', () => {
    const r = sanitizeThemeCss(
      'a { background: linear-gradient(red, blue), url(https://evil.example/1), url(data:,x); }'
    );
    expect(r.css).toBe(
      'a { background: linear-gradient(red, blue), url("data:,"), url(data:,x); }'
    );
    expect(sanitizeThemeCss('a { b: var(--x, url(https://evil.example/x)); }').css).toBe(
      'a { b: var(--x, url("data:,")); }'
    );
  });

  it('drops @import, also with escapes', () => {
    expect(clean('@import url(https://evil.example/a.css);\na { color: red; }')).toBe(
      '\na { color: red; }'
    );
    expect(clean('@import "https://evil.example/a;b.css" screen; a { color: red; }')).toBe(
      ' a { color: red; }'
    );
    expect(clean('@\\69mport "x.css"; a{}')).toBe(' a{}');
  });

  it('blocks image-set, image, src, cross-fade and expression', () => {
    for (const fn of [
      'image-set("https://evil.example/a.png" 1x)',
      '-webkit-image-set("https://evil.example/a.png" 1x)',
      'image("https://evil.example/a.png")',
      'cross-fade(url(data:,x), "https://evil.example/a.png", 50%)',
      'expression(alert(1))'
    ]) {
      const r = sanitizeThemeCss(`a { background: ${fn}; }`);
      expect(r.css, fn).toBe('a { background: blocked-by-chord(); }');
    }
  });

  it('blocks an attr() that builds a url', () => {
    expect(clean('a { background: attr(data-x type(<url>)); }')).toBe(
      'a { background: blocked-by-chord(); }'
    );
    expect(clean('a::before { content: attr(data-x); }')).toBe('a::before { content: attr(data-x); }');
  });

  it('drops behavior and -moz-binding', () => {
    expect(clean('a { behavior: url(x.htc); color: red; }')).toBe('a {  color: red; }');
    expect(clean('a { -moz-binding: url(https://e.example/x.xml) }')).toBe('a { }');
  });

  it('does not loop or throw on broken CSS', () => {
    for (const css of [
      'a { background: url(https://evil.example',
      'a { background: url("https://evil.example',
      '@import',
      '@import url(',
      '/* open comment url(https://e.example)',
      'a { content: "open',
      '\\',
      'url(',
      'a { b: c(((( }'
    ]) {
      expect(() => sanitizeThemeCss(css), css).not.toThrow();
    }
    // Nothing that is left can reach a host.
    expect(clean('a { background: url(https://evil.example')).not.toContain('evil');
  });

  it('reports what it removed', () => {
    const r = sanitizeThemeCss('@import url(https://e.example/a.css); a { b: url(https://f.example/x); }');
    expect(r.blocked).toHaveLength(2);
    expect(r.blocked[1]).toBe('url(https://f.example/x)');
  });
});
