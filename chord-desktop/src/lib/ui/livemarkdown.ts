// Styles for the message box as the user types. It splits the draft into runs, and each
// run gets CSS classes. The runs join back into the exact draft, character for
// character, because the styled copy lies under the real text box and must line up.
// The marks (**, ~~, > and so on) stay visible, in the muted colour.

export interface Run {
  text: string;
  /** Space-separated classes: mk (a mark), b, i, u, s, code, sp (spoiler), link, ts
   *  (timestamp), at (mention), sc (shortcode), h (heading), sub (subtext), q (quote). */
  cls: string;
}

const join = (...c: string[]) => c.filter(Boolean).join(' ');

function push(out: Run[], text: string, cls: string) {
  if (!text) return;
  const last = out[out.length - 1];
  if (last && last.cls === cls) last.text += text;
  else out.push({ text, cls });
}

// Paired marks: [pattern, class]. The order matters: *** before ** before *.
const PAIRS: [RegExp, string, number][] = [
  [/^\*\*\*(?=\S)([\s\S]*?\S)\*\*\*/, 'b i', 3],
  [/^\*\*(?=\S)([\s\S]*?\S)\*\*/, 'b', 2],
  [/^__(?=\S)([\s\S]*?\S)__(?!_)/, 'u', 2],
  [/^~~(?=\S)([\s\S]*?\S)~~/, 's', 2],
  [/^\|\|([\s\S]+?)\|\|/, 'sp', 2],
  [/^\*(?=[^\s*])([^*]*?[^\s*])\*(?!\*)/, 'i', 1],
  [/^\*(?=[^\s*])([^\s*])\*(?!\*)/, 'i', 1]
];

/** Inline styles for one piece of text. `cls` holds the classes from outside. */
function inline(s: string, cls: string, out: Run[]) {
  let i = 0;
  let plain = '';
  const flush = () => {
    push(out, plain, cls);
    plain = '';
  };
  while (i < s.length) {
    const rest = s.slice(i);
    const prev = i > 0 ? s[i - 1] : '';
    let m: RegExpMatchArray | null;

    // An escape: the backslash is a mark, the next character is plain.
    if ((m = rest.match(/^\\([\\*_~`|>#\-[\]():<@])/))) {
      flush();
      push(out, '\\', join(cls, 'mk'));
      push(out, m[1], cls);
      i += 2;
      continue;
    }
    // Inline code: nothing inside it is markdown.
    if ((m = rest.match(/^(``?)([^`\n]|[^`\n][\s\S]*?[^`\n])\1(?!`)/))) {
      flush();
      push(out, m[1], join(cls, 'mk code'));
      push(out, m[2], join(cls, 'code'));
      push(out, m[1], join(cls, 'mk code'));
      i += m[0].length;
      continue;
    }
    // Italic with underscores: not inside a word, so snake_case stays plain.
    if (!/\w/.test(prev) && (m = rest.match(/^_(?=[^\s_])([^_\n]*?[^\s_])_(?!\w)/))) {
      flush();
      push(out, '_', join(cls, 'mk i'));
      inline(m[1], join(cls, 'i'), out);
      push(out, '_', join(cls, 'mk i'));
      i += m[0].length;
      continue;
    }
    let paired = false;
    for (const [re, style, n] of PAIRS) {
      if ((m = rest.match(re))) {
        const mark = m[0].slice(0, n);
        flush();
        push(out, mark, join(cls, 'mk', style));
        inline(m[1], join(cls, style), out);
        push(out, mark, join(cls, 'mk', style));
        i += m[0].length;
        paired = true;
        break;
      }
    }
    if (paired) continue;
    // A timestamp: <t:1735689600> or <t:1735689600:R>.
    if ((m = rest.match(/^<t:-?\d{1,13}(?::[tTdDfFR])?>/))) {
      flush();
      push(out, m[0], join(cls, 'ts'));
      i += m[0].length;
      continue;
    }
    // A masked link: [text](https://...).
    if ((m = rest.match(/^\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/))) {
      flush();
      push(out, '[', join(cls, 'mk'));
      inline(m[1], join(cls, 'link'), out);
      push(out, `](${m[2]})`, join(cls, 'mk'));
      i += m[0].length;
      continue;
    }
    // A link in angle brackets (no preview), or a bare link.
    if ((m = rest.match(/^<https?:\/\/[^\s>]+>/) ?? rest.match(/^https?:\/\/[^\s<]+[^\s<.,;:!?)"']/))) {
      flush();
      push(out, m[0], join(cls, 'link'));
      i += m[0].length;
      continue;
    }
    // A mention, at the start or after a space.
    if ((!prev || /\s/.test(prev)) && (m = rest.match(/^@[\w.-]+/))) {
      flush();
      push(out, m[0], join(cls, 'at'));
      i += m[0].length;
      continue;
    }
    // An emoji shortcode: :name:.
    if ((m = rest.match(/^:[a-z0-9_+-]{2,}:/i))) {
      flush();
      push(out, m[0], join(cls, 'sc'));
      i += m[0].length;
      continue;
    }
    plain += s[i];
    i += 1;
  }
  flush();
}

/** One line outside a code block: the line marks first, then inline styles. */
function line(text: string, inQuote: boolean, out: Run[]): boolean {
  let m: RegExpMatchArray | null;
  let quote = inQuote;
  let rest = text;
  let base = quote ? 'q' : '';
  if (!quote && (m = rest.match(/^>>> /))) {
    push(out, m[0], 'mk q');
    rest = rest.slice(m[0].length);
    quote = true;
    base = 'q';
  } else if ((m = rest.match(/^> /))) {
    push(out, m[0], join(base, 'mk q'));
    rest = rest.slice(m[0].length);
    base = join(base, 'q');
  }
  if ((m = rest.match(/^(#{1,3}) /))) {
    push(out, m[0], join(base, 'mk h'));
    inline(rest.slice(m[0].length), join(base, 'h'), out);
  } else if ((m = rest.match(/^-# /))) {
    push(out, m[0], join(base, 'mk sub'));
    inline(rest.slice(m[0].length), join(base, 'sub'), out);
  } else if ((m = rest.match(/^(\s*)([-*]|\d{1,9}\.) /))) {
    push(out, m[0], join(base, 'mk'));
    inline(rest.slice(m[0].length), base, out);
  } else {
    inline(rest, base, out);
  }
  return quote;
}

/** The styled runs of a draft. The texts of the runs join back into `draft`. */
export function highlightDraft(draft: string): Run[] {
  const out: Run[] = [];
  const lines = draft.split('\n');
  let inCode = false;
  let inQuote = false;
  lines.forEach((text, n) => {
    if (n > 0) push(out, '\n', inCode ? 'code' : inQuote ? 'q' : '');
    const fence = text.match(/^```(\S*)/);
    if (inCode) {
      if (fence && text.trimEnd() === '```') {
        push(out, text, 'mk code');
        inCode = false;
      } else {
        push(out, text, 'code');
      }
      return;
    }
    if (fence) {
      const at = text.indexOf('```', 3);
      if (at >= 0) {
        // A one-line block: ```code```.
        push(out, '```', 'mk code');
        push(out, text.slice(3, at), 'code');
        push(out, text.slice(at, at + 3), 'mk code');
        inline(text.slice(at + 3), '', out);
      } else {
        push(out, text, 'mk code');
        inCode = true;
      }
      return;
    }
    inQuote = line(text, inQuote, out);
  });
  return out;
}
