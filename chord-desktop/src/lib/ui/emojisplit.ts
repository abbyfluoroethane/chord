// Split text into text runs and emoji. Plain code with no app state, so tests can use it.

/** A text run or one emoji. */
export type EmojiPart = { emoji: false; text: string } | { emoji: true; text: string };

let pattern: RegExp | null | undefined;
/** Unicode emoji sequences (RGI). Engines without the `v` flag get a close match. */
function emojiPattern(): RegExp | null {
  if (pattern !== undefined) return pattern;
  try {
    pattern = new RegExp('\\p{RGI_Emoji}', 'gv');
  } catch {
    try {
      pattern = new RegExp(
        '(?:\\p{Regional_Indicator}{2})|(?:[#*0-9]\\uFE0F?\\u20E3)|(?:\\p{Extended_Pictographic}(?:\\uFE0F|\\p{Emoji_Modifier})?(?:\\u200D\\p{Extended_Pictographic}(?:\\uFE0F|\\p{Emoji_Modifier})?)*)',
        'gu'
      );
    } catch {
      pattern = null;
    }
  }
  return pattern;
}

/** Split text into text runs and emoji. */
export function splitEmoji(text: string): EmojiPart[] {
  const re = emojiPattern();
  if (!re || !text) return [{ emoji: false, text }];
  const out: EmojiPart[] = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > last) out.push({ emoji: false, text: text.slice(last, i) });
    out.push({ emoji: true, text: m[0] });
    last = i + m[0].length;
  }
  if (last < text.length) out.push({ emoji: false, text: text.slice(last) });
  return out;
}

/**
 * A saved status as its emoji and its text. A status that starts with one emoji and a
 * space (or is only one emoji) gives that emoji. The status menu saves "$EMOJI $TEXT".
 */
export function splitStatus(status: string | null): { emoji: string; text: string } {
  const s = status?.trim() ?? '';
  const parts = splitEmoji(s);
  const first = parts[0];
  if (first?.emoji && (parts.length === 1 || parts[1].text.startsWith(' '))) {
    return { emoji: first.text, text: s.slice(first.text.length).trim() };
  }
  return { emoji: '', text: s };
}

/** The status to save: the emoji, a space, then the text. Either part can be empty. */
export function joinStatus(emoji: string, text: string): string {
  const t = text.trim();
  return emoji ? (t ? `${emoji} ${t}` : emoji) : t;
}
