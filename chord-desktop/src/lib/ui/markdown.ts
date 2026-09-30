// Turns a message body into a tree, with the rules of Discord chat markdown. The parser is
// pure: it has no DOM. The renderer draws the tree with Svelte markup, so no HTML is injected.

import { dateOf, type TimeStyle } from './timestamp';
import { replaceShortcodes, type Shortcodes } from './shortcodes';

export type Inline =
  | { t: 'text'; v: string }
  | { t: 'code'; v: string }
  | { t: 'link'; href: string; children: Inline[]; masked: boolean; preview: boolean }
  | { t: 'mention'; v: string; me: boolean }
  | { t: 'time'; seconds: number; style: TimeStyle }
  | { t: 'bold' | 'italic' | 'underline' | 'strike' | 'spoiler'; children: Inline[] };

export interface ListItem {
  content: Inline[];
  sub: Block | null;
}

export type Block =
  | { t: 'para'; children: Inline[] }
  | { t: 'code'; lang: string; v: string }
  | { t: 'quote'; children: Block[] }
  | { t: 'heading'; level: 1 | 2 | 3; children: Inline[] }
  | { t: 'subtext'; children: Inline[] }
  | { t: 'list'; ordered: boolean; start: number; items: ListItem[] };

export interface Parsed {
  blocks: Block[];
  /** The message holds only 1 to 30 emoji. Chat shows them large. */
  jumbo: boolean;
}

export interface ParseOptions {
  /** Names that make an @mention count as "me". */
  myNames?: string[];
  /** Known shortcodes. Without them, :name: stays as text. */
  shortcodes?: Shortcodes | null;
}

interface Ctx {
  mine: string[];
  codes: Shortcodes | null;
  inLink: boolean;
}

const ESCAPABLE = /[\\`*_~|[\]()<>#@:!.>+-]/;
const WORD = /[\p{L}\p{N}]/u;
const HTTP = /^https?:\/\/\S+$/i;

// ---------------------------------------------------------------- inline

function runLength(s: string, i: number, c: string): number {
  let n = 0;
  while (s[i + n] === c) n++;
  return n;
}

/** A code span that opens at `i`. The closing run must have the same length. */
function codeSpan(s: string, i: number): { v: string; end: number } | null {
  const n = runLength(s, i, '`');
  let j = i + n;
  while (j < s.length) {
    const at = s.indexOf('`', j);
    if (at < 0) return null;
    const m = runLength(s, at, '`');
    if (m === n) {
      let v = s.slice(i + n, at);
      if (v.length > 2 && v.startsWith(' ') && v.endsWith(' ') && v.trim()) v = v.slice(1, -1);
      return { v, end: at + n };
    }
    j = at + m;
  }
  return null;
}

/** A masked link `[text](url)` that opens at `i`. */
function maskedLink(s: string, i: number): { text: string; href: string; end: number } | null {
  let depth = 0;
  let j = i;
  for (; j < s.length; j++) {
    const c = s[j];
    if (c === '\\') j++;
    else if (c === '[') depth++;
    else if (c === ']' && --depth === 0) break;
  }
  if (j >= s.length || s[j + 1] !== '(') return null;
  const text = s.slice(i + 1, j);
  let k = j + 2;
  let open = 1;
  for (; k < s.length; k++) {
    if (s[k] === '(') open++;
    else if (s[k] === ')' && --open === 0) break;
    else if (s[k] === '\n') return null;
  }
  if (k >= s.length || !text.trim()) return null;
  let href = s.slice(j + 2, k).trim();
  href = href.replace(/\s+"[^"]*"$/, '');
  if (href.startsWith('<') && href.endsWith('>')) href = href.slice(1, -1);
  if (!HTTP.test(href)) return null;
  return { text, href, end: k + 1 };
}

/** A bare link that starts at `i`. Punctuation at the end is not part of the link. */
function bareLink(s: string, i: number): { href: string; end: number } | null {
  const m = /^https?:\/\/[^\s<>]+/i.exec(s.slice(i, i + 2000));
  if (!m) return null;
  let url = m[0];
  for (;;) {
    const last = url[url.length - 1];
    const balanced = last === ')' && url.split('(').length >= url.split(')').length;
    if (balanced || !/[.,;:!?)"'*_~|]/.test(last)) break;
    url = url.slice(0, -1);
  }
  return /^https?:\/\/[^\s]/i.test(url) && url.length > 8 ? { href: url, end: i + url.length } : null;
}

/** The end of the closing marker for an emphasis that opens at `i`, or -1. */
function findCloser(s: string, from: number, c: string, n: number): number {
  let j = from;
  while (j < s.length) {
    const ch = s[j];
    if (ch === '\\') {
      j += 2;
      continue;
    }
    if (ch === '`') {
      const span = codeSpan(s, j);
      j = span ? span.end : j + runLength(s, j, '`');
      continue;
    }
    if (ch !== c) {
      j++;
      continue;
    }
    const r = runLength(s, j, c);
    let at = -1;
    if (n === 1) {
      if (r === 1) at = j;
      else if (r === 3 && c === '*') at = j + 2;
    } else if (n === 2) {
      if (r === 2 || r > 3) at = j;
      else if (r === 3) at = j + 1;
    } else if (r >= 3) at = j;
    if (at >= 0 && closerOk(s, from, at, c, n)) return at;
    j += r;
  }
  return -1;
}

function closerOk(s: string, from: number, at: number, c: string, n: number): boolean {
  const content = s.slice(from, at);
  if (!content) return false;
  if (n === 1 && (/^\s/.test(content) || /\s$/.test(content))) return false;
  if (c === '_' && WORD.test(s[at + n] ?? '')) return false;
  return true;
}

const KIND: Record<string, Array<'italic' | 'bold' | 'underline' | 'strike' | 'spoiler'>> = {
  '*1': ['italic'],
  '*2': ['bold'],
  '*3': ['bold', 'italic'],
  '_1': ['italic'],
  '_2': ['underline'],
  '_3': ['underline', 'italic'],
  '~2': ['strike'],
  '|2': ['spoiler']
};

function emphasis(s: string, i: number, ctx: Ctx): { node: Inline; end: number } | null {
  const c = s[i];
  const run = runLength(s, i, c);
  if ((c === '~' || c === '|') && run < 2) return null;
  if (c === '_' && i > 0 && WORD.test(s[i - 1])) return null;
  const top = c === '~' || c === '|' ? 2 : Math.min(run, 3);
  for (let n = top; n >= 1; n--) {
    if ((c === '~' || c === '|') && n !== 2) continue;
    const at = findCloser(s, i + n, c, n);
    if (at < 0) continue;
    let children = parseInline(s.slice(i + n, at), ctx);
    let node: Inline = { t: 'text', v: '' };
    for (const kind of [...KIND[`${c}${n}`]].reverse()) {
      node = { t: kind, children };
      children = [node];
    }
    return { node, end: at + n };
  }
  return null;
}

export function parseInline(s: string, ctx: Ctx): Inline[] {
  const out: Inline[] = [];
  let buf = '';
  const flush = () => {
    if (!buf) return;
    out.push({ t: 'text', v: ctx.codes ? replaceShortcodes(buf, ctx.codes) : buf });
    buf = '';
  };
  const put = (node: Inline, end: number) => {
    flush();
    out.push(node);
    return end;
  };
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === '\\' && i + 1 < s.length && ESCAPABLE.test(s[i + 1])) {
      buf += s[i + 1];
      i += 2;
    } else if (c === '`') {
      const span = codeSpan(s, i);
      if (span) i = put({ t: 'code', v: span.v }, span.end);
      else {
        const n = runLength(s, i, '`');
        buf += s.slice(i, i + n);
        i += n;
      }
    } else if (c === '<') {
      const rest = s.slice(i, i + 2100);
      const link = /^<(https?:\/\/[^\s<>]+)>/i.exec(rest);
      const time = /^<t:(-?\d{1,13})(?::([tTdDfFR]))?>/.exec(rest);
      const date = time ? dateOf(Number(time[1])) : null;
      if (link && !ctx.inLink) {
        i = put(
          { t: 'link', href: link[1], children: [{ t: 'text', v: link[1] }], masked: false, preview: false },
          i + link[0].length
        );
      } else if (time && date) {
        i = put(
          { t: 'time', seconds: Number(time[1]), style: (time[2] ?? 'f') as TimeStyle },
          i + time[0].length
        );
      } else buf += s[i++];
    } else if (c === '[' && !ctx.inLink) {
      const m = maskedLink(s, i);
      if (m) {
        const children = parseInline(m.text, { ...ctx, inLink: true });
        i = put({ t: 'link', href: m.href, children, masked: true, preview: true }, m.end);
      } else buf += s[i++];
    } else if ((c === 'h' || c === 'H') && !ctx.inLink && (i === 0 || !WORD.test(s[i - 1]))) {
      const m = bareLink(s, i);
      if (m) {
        i = put(
          { t: 'link', href: m.href, children: [{ t: 'text', v: m.href }], masked: false, preview: true },
          m.end
        );
      } else buf += s[i++];
    } else if (c === '@' && (i === 0 || !/[\w.]/.test(s[i - 1]))) {
      const m = /^@[\w.-]*\w/.exec(s.slice(i, i + 200));
      if (m) {
        const me = ctx.mine.includes(m[0].slice(1).toLowerCase());
        i = put({ t: 'mention', v: m[0], me }, i + m[0].length);
      } else buf += s[i++];
    } else if (c === '*' || c === '_' || c === '~' || c === '|') {
      const m = emphasis(s, i, ctx);
      if (m) i = put(m.node, m.end);
      else {
        // An unclosed marker stays as text. The whole run stays, so it does not open later.
        const n = runLength(s, i, c);
        buf += s.slice(i, i + n);
        i += n;
      }
    } else buf += s[i++];
  }
  flush();
  return out;
}

// ---------------------------------------------------------------- blocks

const LIST = /^(\s*)([-*]|\d{1,9}\.) (.*)$/;

function readList(lines: string[], from: number, indent: number, ctx: Ctx): [Block, number] {
  const first = LIST.exec(lines[from])!;
  const ordered = /\d/.test(first[2]);
  const items: ListItem[] = [];
  let i = from;
  while (i < lines.length) {
    const m = LIST.exec(lines[i]);
    if (!m || m[1].length < indent) break;
    if (m[1].length > indent && items.length) {
      const [sub, next] = readList(lines, i, m[1].length, ctx);
      items[items.length - 1].sub = sub;
      i = next;
      continue;
    }
    if (/\d/.test(m[2]) !== ordered) break;
    items.push({ content: parseInline(m[3], ctx), sub: null });
    i++;
  }
  return [{ t: 'list', ordered, start: ordered ? parseInt(first[2], 10) : 1, items }, i];
}

function parseBlocks(text: string, ctx: Ctx, quotes: boolean): Block[] {
  const blocks: Block[] = [];
  let para: string[] = [];
  const flush = () => {
    const joined = para.join('\n').replace(/^\n+|\n+$/g, '');
    para = [];
    if (joined.trim()) blocks.push({ t: 'para', children: parseInline(joined, ctx) });
  };
  let pos = 0;
  while (pos < text.length) {
    const eol = text.indexOf('\n', pos);
    const end = eol < 0 ? text.length : eol;
    const line = text.slice(pos, end);
    let m: RegExpExecArray | null;

    if (line.startsWith('```')) {
      const close = text.indexOf('```', pos + 3);
      if (close > pos + 3) {
        let body = text.slice(pos + 3, close);
        let lang = '';
        const head = /^([A-Za-z0-9_+#.-]*)\n([\s\S]*)$/.exec(body);
        if (head) {
          lang = head[1];
          body = head[2];
        }
        body = body.replace(/\n$/, '');
        if (body || lang) {
          flush();
          blocks.push({ t: 'code', lang: lang.toLowerCase(), v: body });
          pos = close + 3;
          if (text[pos] === '\n') pos++;
          continue;
        }
      }
    }
    if (quotes && line.startsWith('>>> ')) {
      flush();
      const rest = line.slice(4) + (eol < 0 ? '' : '\n' + text.slice(eol + 1));
      blocks.push({ t: 'quote', children: parseBlocks(rest, ctx, false) });
      break;
    }
    if (quotes && line.startsWith('> ')) {
      flush();
      const inner: string[] = [];
      let p = pos;
      while (p < text.length) {
        const e = text.indexOf('\n', p);
        const l = text.slice(p, e < 0 ? text.length : e);
        if (l.startsWith('> ')) inner.push(l.slice(2));
        else if (l === '>') inner.push('');
        else break;
        p = e < 0 ? text.length : e + 1;
      }
      blocks.push({ t: 'quote', children: parseBlocks(inner.join('\n'), ctx, false) });
      pos = p;
      continue;
    }
    if ((m = /^(#{1,3}) (.+)$/.exec(line))) {
      flush();
      blocks.push({ t: 'heading', level: m[1].length as 1 | 2 | 3, children: parseInline(m[2], ctx) });
    } else if ((m = /^-# (.+)$/.exec(line))) {
      flush();
      blocks.push({ t: 'subtext', children: parseInline(m[1], ctx) });
    } else if (LIST.test(line)) {
      flush();
      const lines: string[] = [];
      let p = pos;
      while (p < text.length) {
        const e = text.indexOf('\n', p);
        const l = text.slice(p, e < 0 ? text.length : e);
        if (!LIST.test(l)) break;
        lines.push(l);
        p = e < 0 ? text.length : e + 1;
      }
      const [list] = readList(lines, 0, LIST.exec(lines[0])![1].length, ctx);
      blocks.push(list);
      pos = p;
      continue;
    } else para.push(line);
    pos = end + 1;
  }
  flush();
  return blocks;
}

// ---------------------------------------------------------------- jumbo

const EMOJI =
  /\p{Regional_Indicator}{2}|[#*0-9]️?⃣|\p{Extended_Pictographic}(?:️|\p{Emoji_Modifier})?(?:‍\p{Extended_Pictographic}(?:️|\p{Emoji_Modifier})?)*[\u{E0020}-\u{E007F}]*/gu;

function isJumbo(blocks: Block[]): boolean {
  if (blocks.length !== 1 || blocks[0].t !== 'para') return false;
  const kids = blocks[0].children;
  if (!kids.every((k) => k.t === 'text')) return false;
  const text = kids.map((k) => (k as { v: string }).v).join('').replace(/\s+/g, '');
  const found = text.match(EMOJI) ?? [];
  return found.length >= 1 && found.length <= 30 && found.join('') === text;
}

// ---------------------------------------------------------------- entry points

/** Parse a message body. */
export function parseMarkdown(body: string, opt: ParseOptions = {}): Parsed {
  const ctx: Ctx = {
    mine: (opt.myNames ?? []).map((n) => n.toLowerCase()),
    codes: opt.shortcodes ?? null,
    inLink: false
  };
  const blocks = parseBlocks(body.replace(/\r\n?/g, '\n'), ctx, true);
  return { blocks, jumbo: isJumbo(blocks) };
}

function walkInline(nodes: Inline[], out: string[]) {
  for (const n of nodes) {
    if (n.t === 'link') {
      if (n.preview) out.push(n.href);
    } else if ('children' in n) walkInline(n.children, out);
  }
}

function walkList(block: Block, out: string[]) {
  if (block.t !== 'list') return;
  for (const item of block.items) {
    walkInline(item.content, out);
    if (item.sub) walkList(item.sub, out);
  }
}

/** The links of a body that get a preview: not `<url>` and not code. */
export function previewUrls(body: string): string[] {
  const out: string[] = [];
  const visit = (blocks: Block[]) => {
    for (const b of blocks) {
      if (b.t === 'quote') visit(b.children);
      else if (b.t === 'list') walkList(b, out);
      else if (b.t !== 'code') walkInline(b.children, out);
    }
  };
  visit(parseMarkdown(body).blocks);
  return out;
}
