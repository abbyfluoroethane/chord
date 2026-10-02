// Message search helpers: match the loaded messages (the browser preview), cut an excerpt
// around the match, name the sender, and jump to a message in the timeline.

import type { SearchHit } from '$lib/chord/types';
import type { TimelineItem } from './types';

/** The most hits that the panel asks for. */
export const SEARCH_LIMIT = 50;

/** A text in three parts: the match sits in the middle one. */
export interface Excerpt {
  before: string;
  match: string;
  after: string;
}

/** Whether `body` contains `query`, ignoring case. A blank query matches nothing. */
export function matches(body: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  return q !== '' && body.toLowerCase().includes(q);
}

/** The hits among the loaded messages, newest first. It works like the core search. */
export function searchItems(items: TimelineItem[], peer: string, query: string): SearchHit[] {
  return items
    .filter((m) => !m.retracted && matches(m.body, query))
    .map((m) => ({
      id: m.id,
      kind: 'chat' as const,
      direction: m.outgoing ? ('out' as const) : ('in' as const),
      peer,
      sender: m.senderName,
      body: m.body,
      timestamp: m.timestamp
    }))
    .reverse()
    .slice(0, SEARCH_LIMIT);
}

/**
 * Cut `body` to about `radius` characters each side of the first match. A body without a
 * match gives its start. Line breaks become spaces.
 */
export function excerpt(body: string, query: string, radius = 40): Excerpt {
  const flat = body.replace(/\s+/g, ' ').trim();
  const q = query.trim().toLowerCase();
  const at = q ? flat.toLowerCase().indexOf(q) : -1;
  if (at < 0) return { before: '', match: '', after: flat.slice(0, radius * 2) };
  const from = Math.max(0, at - radius);
  const to = Math.min(flat.length, at + q.length + radius);
  return {
    before: (from > 0 ? '…' : '') + flat.slice(from, at),
    match: flat.slice(at, at + q.length),
    after: flat.slice(at + q.length, to) + (to < flat.length ? '…' : '')
  };
}

/** Who wrote a hit: "you", the nick in a room, or the local part of the address. */
export function senderLabel(hit: SearchHit): string {
  if (hit.direction === 'out') return 'you';
  if (hit.kind === 'groupchat') return hit.sender.split('/')[1] ?? hit.sender.split('@')[0];
  return hit.sender.split('@')[0].split('/')[0];
}

/** The event that a jump sends to the timeline before it scrolls. */
export const JUMP_EVENT = 'chordjump';

/**
 * Scroll to a message in the timeline and flash it. Returns false when the message is not
 * in the page: it is older than the messages that the timeline holds. The scroll is
 * instant: a smooth scroll passes the top of the list and loads older messages, which
 * move the target.
 */
export function jumpToMessage(id: string): boolean {
  const el = document.getElementById(`msg-${id}`);
  if (!el) return false;
  // The timeline stops following the newest message before the view moves.
  el.dispatchEvent(new CustomEvent(JUMP_EVENT, { bubbles: true }));
  el.scrollIntoView({ block: 'center', behavior: 'instant' });
  el.classList.remove('flash');
  void el.offsetWidth;
  el.classList.add('flash');
  return true;
}
