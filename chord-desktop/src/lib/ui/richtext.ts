// Turns a message body into safe segments. No HTML is ever injected.

export type Segment =
  | { t: 'text'; v: string }
  | { t: 'link'; v: string }
  | { t: 'mention'; v: string; me: boolean }
  | { t: 'code'; v: string };

const TOKEN = /(`[^`\n]+`)|(https?:\/\/[^\s<>]+[^\s<>.,;:!?)"'])|(@[\w.-]+)/g;

export function segments(body: string, myNames: string[]): Segment[] {
  const out: Segment[] = [];
  let last = 0;
  const mine = myNames.map((n) => n.toLowerCase());
  for (const m of body.matchAll(TOKEN)) {
    const i = m.index ?? 0;
    if (i > last) out.push({ t: 'text', v: body.slice(last, i) });
    if (m[1]) out.push({ t: 'code', v: m[1].slice(1, -1) });
    else if (m[2]) out.push({ t: 'link', v: m[2] });
    else out.push({ t: 'mention', v: m[3], me: mine.includes(m[3].slice(1).toLowerCase()) });
    last = i + m[0].length;
  }
  if (last < body.length) out.push({ t: 'text', v: body.slice(last) });
  return out;
}
