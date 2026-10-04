import { describe, expect, it } from 'vitest';
import { parseMarkdown, previewUrls, xmppUrls, type Block, type Inline } from './markdown';

/** The inline nodes of a one-paragraph message. */
function inl(body: string, opt = {}): Inline[] {
  const b = parseMarkdown(body, opt).blocks[0] as Block;
  if (b.t !== 'para') throw new Error(`not a paragraph: ${b.t}`);
  return b.children;
}

const text = (v: string): Inline => ({ t: 'text', v });

describe('inline styles', () => {
  it('reads italic with * and _', () => {
    expect(inl('*a*')).toEqual([{ t: 'italic', children: [text('a')] }]);
    expect(inl('_a_')).toEqual([{ t: 'italic', children: [text('a')] }]);
  });
  it('reads bold, underline, strike and bold italic', () => {
    expect(inl('**a**')).toEqual([{ t: 'bold', children: [text('a')] }]);
    expect(inl('__a__')).toEqual([{ t: 'underline', children: [text('a')] }]);
    expect(inl('~~a~~')).toEqual([{ t: 'strike', children: [text('a')] }]);
    expect(inl('***a***')).toEqual([
      { t: 'bold', children: [{ t: 'italic', children: [text('a')] }] }
    ]);
  });
  it('reads a spoiler', () => {
    expect(inl('||secret||')).toEqual([{ t: 'spoiler', children: [text('secret')] }]);
  });
  it('nests bold inside italic', () => {
    expect(inl('*a **b** c*')).toEqual([
      {
        t: 'italic',
        children: [text('a '), { t: 'bold', children: [text('b')] }, text(' c')]
      }
    ]);
  });
  it('nests italic inside bold at the end', () => {
    expect(inl('**a *b***')).toEqual([
      { t: 'bold', children: [text('a '), { t: 'italic', children: [text('b')] }] }
    ]);
  });
  it('keeps text around a style', () => {
    expect(inl('x **a** y')).toEqual([text('x '), { t: 'bold', children: [text('a')] }, text(' y')]);
  });
  it('does not make italic inside a word with underscores', () => {
    expect(inl('snake_case_name')).toEqual([text('snake_case_name')]);
    expect(inl('a_b_c and __init__x')).toEqual([text('a_b_c and __init__x')]);
  });
  it('keeps an unclosed marker as text', () => {
    expect(inl('**a')).toEqual([text('**a')]);
    expect(inl('a * b * c')).toEqual([text('a * b * c')]);
    expect(inl('||a')).toEqual([text('||a')]);
    expect(inl('`a')).toEqual([text('`a')]);
  });
  it('escapes markdown characters', () => {
    expect(inl('\\*a\\*')).toEqual([text('*a*')]);
    expect(inl('\\_a\\_ and \\`x\\`')).toEqual([text('_a_ and `x`')]);
    expect(inl('a\\b')).toEqual([text('a\\b')]);
  });
});

describe('code', () => {
  it('keeps markdown inside inline code literal', () => {
    expect(inl('`**a** _b_`')).toEqual([{ t: 'code', v: '**a** _b_' }]);
  });
  it('reads a double-tick span that holds a tick', () => {
    expect(inl('`` code with ` ``')).toEqual([{ t: 'code', v: 'code with `' }]);
  });
  it('reads a fenced block with a language', () => {
    const p = parseMarkdown('```rust\nfn main() {}\n```');
    expect(p.blocks).toEqual([{ t: 'code', lang: 'rust', v: 'fn main() {}' }]);
  });
  it('reads a fenced block without a language', () => {
    expect(parseMarkdown('```\na **b**\nc\n```').blocks).toEqual([
      { t: 'code', lang: '', v: 'a **b**\nc' }
    ]);
    expect(parseMarkdown('```one line```').blocks).toEqual([
      { t: 'code', lang: '', v: 'one line' }
    ]);
  });
  it('keeps text before and after a block', () => {
    const b = parseMarkdown('before\n```\nx\n```\nafter').blocks;
    expect(b.map((x) => x.t)).toEqual(['para', 'code', 'para']);
  });
  it('keeps an unclosed fence as text', () => {
    expect(parseMarkdown('```js\nlet a').blocks.map((x) => x.t)).toEqual(['para']);
  });
});

describe('links', () => {
  it('reads a bare link and drops the final punctuation', () => {
    expect(inl('see https://example.org/a, ok')).toEqual([
      text('see '),
      {
        t: 'link',
        href: 'https://example.org/a',
        children: [text('https://example.org/a')],
        masked: false,
        preview: true
      },
      text(', ok')
    ]);
  });
  it('keeps a balanced paren in a link', () => {
    const n = inl('https://example.org/a_(b)')[0];
    expect(n).toMatchObject({ t: 'link', href: 'https://example.org/a_(b)' });
  });
  it('reads a masked link', () => {
    expect(inl('[the **doc**](https://example.org/d)')).toEqual([
      {
        t: 'link',
        href: 'https://example.org/d',
        children: [text('the '), { t: 'bold', children: [text('doc')] }],
        masked: true,
        preview: true
      }
    ]);
  });
  it('does not make a link of a javascript: masked link', () => {
    const out = inl('[click](javascript:alert(1))');
    expect(out.some((n) => n.t === 'link')).toBe(false);
    expect(out).toEqual([text('[click](javascript:alert(1))')]);
    expect(inl('[x](data:text/html,hi)').some((n) => n.t === 'link')).toBe(false);
  });
  it('reads <url> as a link without a preview', () => {
    expect(inl('<https://example.org/x>')).toEqual([
      {
        t: 'link',
        href: 'https://example.org/x',
        children: [text('https://example.org/x')],
        masked: false,
        preview: false
      }
    ]);
  });
  it('lists preview links, not <url> and not code', () => {
    const body =
      'https://a.example.org <https://b.example.org> `https://c.example.org` [t](https://d.example.org)\n```\nhttps://e.example.org\n```';
    expect(previewUrls(body)).toEqual(['https://a.example.org', 'https://d.example.org']);
  });
  it('finds links in quotes and lists', () => {
    expect(previewUrls('> https://a.example.org\n- https://b.example.org')).toEqual([
      'https://a.example.org',
      'https://b.example.org'
    ]);
  });
});

describe('xmpp links', () => {
  const space = 'xmpp:pubsub.chord.example?pubsub;action=subscribe;node=launch-ops';
  const room = 'xmpp:dev@conference.chord.example?join';

  it('reads a bare xmpp: link', () => {
    expect(inl(`Join us: ${space}`)).toEqual([
      text('Join us: '),
      { t: 'link', href: space, children: [text(space)], masked: false, preview: true, xmpp: true }
    ]);
  });
  it('drops the punctuation after the link', () => {
    const n = inl(`Try ${room}.`)[1];
    expect(n).toMatchObject({ t: 'link', href: room });
    expect(inl(`(${room})`)[1]).toMatchObject({ t: 'link', href: room });
  });
  it('reads a masked link and a <uri>', () => {
    expect(inl(`[the room](${room})`)[0]).toMatchObject({
      t: 'link',
      href: room,
      masked: true,
      xmpp: true
    });
    expect(inl(`<${room}>`)[0]).toMatchObject({ t: 'link', href: room, preview: false, xmpp: true });
  });
  it('leaves a bad xmpp: link as text', () => {
    for (const bad of ['xmpp:', 'xmpp:sam@chord.example?remove', 'xmpp:p.example.org?;node=']) {
      expect(inl(`see ${bad}`).some((n) => n.t === 'link')).toBe(false);
    }
    expect(inl('[x](xmpp:sam@chord.example?remove)').some((n) => n.t === 'link')).toBe(false);
  });
  it('needs a word boundary', () => {
    expect(inl('linuxxmpp:sam@chord.example').some((n) => n.t === 'link')).toBe(false);
  });
  it('lists xmpp: links apart from web links', () => {
    const body = `${space} https://a.example.org <${room}> \`${room}\` [r](${room})`;
    expect(xmppUrls(body)).toEqual([space, room]);
    expect(previewUrls(body)).toEqual(['https://a.example.org']);
  });
  it('finds xmpp: links in quotes and lists', () => {
    expect(xmppUrls(`> ${room}\n- ${space}`)).toEqual([room, space]);
  });
});

describe('mentions', () => {
  it('marks a mention of me', () => {
    expect(inl('hi @abby and @rin', { myNames: ['Abby'] })).toEqual([
      text('hi '),
      { t: 'mention', v: '@abby', me: true },
      text(' and '),
      { t: 'mention', v: '@rin', me: false }
    ]);
  });
  it('does not read an address as a mention', () => {
    expect(inl('mail abby@example.org')).toEqual([text('mail abby@example.org')]);
  });
});

describe('timestamps', () => {
  it('reads the default and a set style', () => {
    expect(inl('<t:1735689600>')).toEqual([{ t: 'time', seconds: 1735689600, style: 'f' }]);
    expect(inl('<t:1735689600:R>')).toEqual([{ t: 'time', seconds: 1735689600, style: 'R' }]);
  });
  it('keeps an invalid timestamp as text', () => {
    expect(inl('<t:abc>')).toEqual([text('<t:abc>')]);
    expect(inl('<t:1735689600:X>')).toEqual([text('<t:1735689600:X>')]);
    expect(inl('<t:99999999999999>')).toEqual([text('<t:99999999999999>')]);
  });
});

describe('blocks', () => {
  it('reads headings only with a space', () => {
    expect(parseMarkdown('# One\n## Two\n### Three').blocks.map((b) => b.t === 'heading' && b.level)).toEqual([
      1, 2, 3
    ]);
    expect(parseMarkdown('#One').blocks[0].t).toBe('para');
    expect(parseMarkdown('#### Four').blocks[0].t).toBe('para');
  });
  it('reads subtext', () => {
    expect(parseMarkdown('-# small').blocks).toEqual([{ t: 'subtext', children: [text('small')] }]);
  });
  it('reads a one-line quote and joins lines', () => {
    expect(parseMarkdown('> a\n> b\nc').blocks.map((b) => b.t)).toEqual(['quote', 'para']);
    const q = parseMarkdown('> **a**').blocks[0];
    expect(q).toEqual({
      t: 'quote',
      children: [{ t: 'para', children: [{ t: 'bold', children: [text('a')] }] }]
    });
  });
  it('reads a multi-line quote to the end', () => {
    const b = parseMarkdown('x\n>>> a\nb\n# c').blocks;
    expect(b.map((x) => x.t)).toEqual(['para', 'quote']);
    expect((b[1] as { children: Block[] }).children.map((x) => x.t)).toEqual(['para', 'heading']);
  });
  it('reads lists with nesting', () => {
    const b = parseMarkdown('- a\n  - b\n  - c\n- d').blocks[0];
    expect(b).toMatchObject({ t: 'list', ordered: false });
    if (b.t !== 'list') return;
    expect(b.items).toHaveLength(2);
    expect(b.items[0].sub).toMatchObject({ t: 'list', items: [{}, {}] });
  });
  it('reads an ordered list and a star list', () => {
    const b = parseMarkdown('3. a\n4. **b**').blocks[0];
    expect(b).toMatchObject({ t: 'list', ordered: true, start: 3 });
    expect(parseMarkdown('* a\n* b').blocks[0]).toMatchObject({ t: 'list', ordered: false });
    expect(parseMarkdown('*a*').blocks[0].t).toBe('para');
  });
  it('formats inside a list item', () => {
    const b = parseMarkdown('- **a**').blocks[0];
    if (b.t !== 'list') throw new Error('no list');
    expect(b.items[0].content).toEqual([{ t: 'bold', children: [text('a')] }]);
  });
  it('keeps blank lines inside a paragraph', () => {
    expect(inl('a\n\nb')).toEqual([text('a\n\nb')]);
  });
});

describe('jumbo', () => {
  it('detects a message of emoji only', () => {
    expect(parseMarkdown('😀').jumbo).toBe(true);
    expect(parseMarkdown('😀 👍🏽 🇫🇷 👨‍👩‍👧').jumbo).toBe(true);
    expect(parseMarkdown('1️⃣').jumbo).toBe(true);
  });
  it('rejects text, digits, and more than 30 emoji', () => {
    expect(parseMarkdown('hi 😀').jumbo).toBe(false);
    expect(parseMarkdown('123').jumbo).toBe(false);
    expect(parseMarkdown('😀'.repeat(31)).jumbo).toBe(false);
    expect(parseMarkdown('😀'.repeat(30)).jumbo).toBe(true);
    expect(parseMarkdown('').jumbo).toBe(false);
    expect(parseMarkdown('`😀`').jumbo).toBe(false);
  });
  it('counts a known shortcode as an emoji', () => {
    const codes = new Map([['smile', '😄']]);
    expect(parseMarkdown(':smile:', { shortcodes: codes }).jumbo).toBe(true);
    expect(parseMarkdown(':smile:').jumbo).toBe(false);
  });
});

describe('shortcodes in text', () => {
  const codes = new Map([['smile', '😄']]);
  it('replaces a known shortcode', () => {
    expect(inl('a :smile: b', { shortcodes: codes })).toEqual([text('a 😄 b')]);
  });
  it('keeps an unknown shortcode and code', () => {
    expect(inl(':nope:', { shortcodes: codes })).toEqual([text(':nope:')]);
    expect(inl('`:smile:`', { shortcodes: codes })).toEqual([{ t: 'code', v: ':smile:' }]);
  });
});

describe('speed', () => {
  it('reads a message full of unclosed markers quickly', () => {
    const start = performance.now();
    parseMarkdown('*a '.repeat(3000));
    expect(performance.now() - start).toBeLessThan(2000);
  });
});
