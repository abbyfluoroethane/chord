// What a theme may do. A theme is CSS from another person, and a linked theme can change
// after the user adds it. CSS can send a request: `background: url(https://host/?x)` on a
// selector such as `a[href^="https://bank"]` tells the host which links and file names are
// on screen, one guess at a time (BRIDGESECURITY-05). So Chord removes everything that
// loads a file from outside the app before the CSS reaches the page:
//
//   - `@import` rules,
//   - `url()` of anything but a `data:` URL or a `#fragment`,
//   - `image-set()`, `image()`, `src()`, `cross-fade()` and `expression()`, which can name
//     an address in a string,
//   - `attr()` that builds a `url`,
//   - the `behavior` and `-moz-binding` properties.
//
// The code reads the CSS token by token and decodes the escapes of a name before it looks
// at it, so `u\72l(` is a `url(` for this check, as it is for the browser. Built-in themes
// skip the check. Everything else in the documented theme format (custom properties,
// `[data-accent]` rules, any selector, fonts from the app) keeps working.

export interface SafeTheme {
  css: string;
  /** What was taken out, for the user: short texts like `@import` or `url(https://host/)`. */
  blocked: string[];
}

/** A function name that can load a file from a string. The browser drops the declaration. */
const BLOCKED_FUNCTIONS = new Set([
  'image-set',
  '-webkit-image-set',
  'image',
  'src',
  'cross-fade',
  '-webkit-cross-fade',
  'expression'
]);
const BLOCKED_PROPERTIES = new Set(['behavior', '-moz-binding']);
/** What replaces a `url()` that is not safe. It loads nothing. */
const EMPTY_URL = 'url("data:,")';

const isHex = (c: string) => /[0-9a-fA-F]/.test(c);
const isWs = (c: string) => c === ' ' || c === '\t' || c === '\n' || c === '\r' || c === '\f';
const isIdentStart = (c: string) => /[A-Za-z_\u0080-￿\\-]/.test(c);
const isIdentChar = (c: string) => /[A-Za-z0-9_\u0080-￿\\-]/.test(c);

/** Read one escape that starts at `i` (a backslash). Returns the character and the next index. */
function escape(css: string, i: number): [string, number] {
  let j = i + 1;
  if (j >= css.length) return ['�', j];
  if (isHex(css[j])) {
    let hex = '';
    while (j < css.length && hex.length < 6 && isHex(css[j])) hex += css[j++];
    if (j < css.length && isWs(css[j])) {
      // A CRLF pair counts as one white space.
      j += css[j] === '\r' && css[j + 1] === '\n' ? 2 : 1;
    }
    const code = parseInt(hex, 16);
    return [code === 0 || code > 0x10ffff ? '�' : String.fromCodePoint(code), j];
  }
  return [css[j], j + 1];
}

/** Read a name at `i`. Returns the raw text, the decoded text and the next index. */
function ident(css: string, i: number): { raw: string; name: string; end: number } {
  let j = i;
  let name = '';
  while (j < css.length && isIdentChar(css[j])) {
    if (css[j] === '\\') {
      const [ch, next] = escape(css, j);
      name += ch;
      j = next;
    } else name += css[j++];
  }
  return { raw: css.slice(i, j), name: name.toLowerCase(), end: j };
}

/** Skip a string that starts at `i` (a quote). Returns its decoded value and the next index. */
function readString(css: string, i: number): { value: string; end: number } {
  const quote = css[i];
  let j = i + 1;
  let value = '';
  while (j < css.length && css[j] !== quote) {
    if (css[j] === '\\') {
      if (css[j + 1] === '\n') {
        j += 2;
        continue;
      }
      const [ch, next] = escape(css, j);
      value += ch;
      j = next;
    } else if (css[j] === '\n') {
      break; // a bad string ends at the line end
    } else value += css[j++];
  }
  return { value, end: Math.min(j + 1, css.length) };
}

/** True if the decoded value of a `url()` is one that cannot reach the network. */
function safeUrl(value: string): boolean {
  const v = value.trim();
  return v.startsWith('#') || /^data:/i.test(v);
}

/** The text of a blocked URL for the user, cut to 60 characters. */
function shown(value: string): string {
  const v = value.trim().replace(/\s+/g, ' ');
  return v.length > 60 ? v.slice(0, 57) + '...' : v;
}

/** Find the `)` that ends a function whose `(` is at `open`. Skips strings and comments. */
function closing(css: string, open: number): number {
  let depth = 0;
  for (let j = open; j < css.length; j++) {
    const c = css[j];
    if (c === '"' || c === "'") j = readString(css, j).end - 1;
    else if (c === '/' && css[j + 1] === '*') {
      const end = css.indexOf('*/', j + 2);
      j = end < 0 ? css.length : end + 1;
    } else if (c === '\\') j = escape(css, j)[1] - 1;
    else if (c === '(') depth++;
    else if (c === ')' && --depth === 0) return j;
  }
  return css.length - 1;
}

export function sanitizeThemeCss(css: string): SafeTheme {
  const out: string[] = [];
  const blocked: string[] = [];
  let i = 0;
  while (i < css.length) {
    const c = css[i];
    // A comment stays as it is. The header of the theme is one.
    if (c === '/' && css[i + 1] === '*') {
      const end = css.indexOf('*/', i + 2);
      const stop = end < 0 ? css.length : end + 2;
      out.push(css.slice(i, stop));
      i = stop;
      continue;
    }
    if (c === '"' || c === "'") {
      const { end } = readString(css, i);
      out.push(css.slice(i, end));
      i = end;
      continue;
    }
    if (c === '@') {
      const id = ident(css, i + 1);
      if (id.name === 'import') {
        // Drop the whole rule, up to its `;`. A string or a `url()` in it can hold a `;`.
        let j = id.end;
        while (j < css.length && css[j] !== ';' && css[j] !== '{') {
          if (css[j] === '"' || css[j] === "'") j = readString(css, j).end;
          else if (css[j] === '(') j = closing(css, j) + 1;
          else j++;
        }
        blocked.push('@import ' + shown(css.slice(id.end, j)));
        i = css[j] === ';' ? j + 1 : j;
        continue;
      }
      out.push('@' + id.raw);
      i = id.end;
      continue;
    }
    // A name, also one that starts with an escape. A lone `-` is an operator.
    if (isIdentStart(c) && !(c === '-' && !isIdentChar(css[i + 1] ?? ' '))) {
      const id = ident(css, i);
      i = handleIdent(id.raw, id.name, id.end);
      continue;
    }
    out.push(c);
    i++;
  }
  return { css: out.join(''), blocked };

  /** One name: a function to check, a property to drop, or plain text. Returns the next index. */
  function handleIdent(raw: string, name: string, end: number): number {
    if (css[end] === '(') {
      const close = closing(css, end);
      if (name === 'url') {
        const inner = css.slice(end + 1, close).trim();
        // An unquoted URL can hold escapes as well.
        const value =
          inner[0] === '"' || inner[0] === "'" ? readString(inner, 0).value : decodeAll(inner);
        if (safeUrl(value)) out.push(css.slice(end - raw.length, close + 1));
        else {
          blocked.push('url(' + shown(value) + ')');
          out.push(EMPTY_URL);
        }
        return close + 1;
      }
      if (BLOCKED_FUNCTIONS.has(name)) {
        blocked.push(name + '()');
        // Skip the whole call and leave a function the browser does not know.
        out.push('blocked-by-chord()');
        return close + 1;
      }
      if (name === 'attr' && /url/i.test(decodeAll(css.slice(end + 1, close)))) {
        blocked.push('attr() with a url');
        out.push('blocked-by-chord()');
        return close + 1;
      }
      out.push(raw);
      return end; // the `(` and its content go through the loop: a nested url() is found
    }
    if (BLOCKED_PROPERTIES.has(name)) {
      let j = end;
      while (j < css.length && isWs(css[j])) j++;
      if (css[j] === ':') {
        // Drop the declaration, up to `;` or the `}` of the block.
        let k = j;
        while (k < css.length && css[k] !== ';' && css[k] !== '}') {
          if (css[k] === '"' || css[k] === "'") k = readString(css, k).end;
          else if (css[k] === '(') k = closing(css, k) + 1;
          else k++;
        }
        blocked.push(name);
        return css[k] === ';' ? k + 1 : k;
      }
    }
    out.push(raw);
    return end;
  }
}

/** Decode every escape of a short text. */
function decodeAll(text: string): string {
  let s = '';
  for (let i = 0; i < text.length; ) {
    if (text[i] === '\\') {
      const [ch, next] = escape(text, i);
      s += ch;
      i = next;
    } else s += text[i++];
  }
  return s;
}
