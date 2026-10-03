// Text emoticons such as :) become emoji when the user sends a message. Only a whole word
// counts, so a link such as http://x or the text a:b stays as typed. Code stays as typed.
import { CODE_SPANS } from './shortcodes';

const MAP: Record<string, string> = {
  ':)': '🙂',
  ':-)': '🙂',
  ':D': '😄',
  ':-D': '😄',
  ';)': '😉',
  ';-)': '😉',
  ':(': '🙁',
  ':-(': '🙁',
  ":'(": '😢',
  ':P': '😛',
  ':-P': '😛',
  ':p': '😛',
  ':O': '😮',
  ':-O': '😮',
  ':o': '😮',
  ':/': '😕',
  ':-/': '😕',
  ':|': '😐',
  ':-|': '😐',
  'B)': '😎',
  'B-)': '😎',
  '<3': '❤️',
  XD: '😆'
};

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const WORD = new RegExp(
  `(^|\\s)(${Object.keys(MAP)
    .sort((a, b) => b.length - a.length)
    .map(escape)
    .join('|')})(?=$|\\s|[.,!?])`,
  'g'
);

/** Replace each emoticon that stands alone as a word. Code stays as typed. */
export function replaceEmoticons(text: string): string {
  return text
    .split(CODE_SPANS)
    .map((part, i) => (i % 2 === 1 ? part : part.replace(WORD, (_, pre: string, e: string) => pre + MAP[e])))
    .join('');
}
