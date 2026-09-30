import { describe, expect, it } from 'vitest';
import { escMarksRead, pageTitle, totalUnread, unreadChannels } from './unread';

describe('unread totals', () => {
  it('adds the unread of the chats that are not muted', () => {
    expect(
      totalUnread([
        { unread: 2, muted: false },
        { unread: 5, muted: true },
        { unread: 1, muted: false }
      ])
    ).toBe(3);
  });

  it('shows the count in the title only when it is above zero', () => {
    expect(pageTitle(0)).toBe('Chord');
    expect(pageTitle(7)).toBe('(7) Chord');
  });

  it('picks the channels with unread messages or mentions', () => {
    const list = [
      { unread: 0, mentions: 0 },
      { unread: 1, mentions: 0 },
      { unread: 0, mentions: 2 }
    ];
    expect(unreadChannels(list)).toHaveLength(2);
  });
});

describe('Esc', () => {
  const el = (tagName: string, isContentEditable = false) =>
    ({ tagName, isContentEditable }) as unknown as EventTarget;

  it('does not mark read in a text field', () => {
    expect(escMarksRead(el('TEXTAREA'))).toBe(false);
    expect(escMarksRead(el('INPUT'))).toBe(false);
    expect(escMarksRead(el('DIV', true))).toBe(false);
  });

  it('marks read elsewhere', () => {
    expect(escMarksRead(el('BODY'))).toBe(true);
    expect(escMarksRead(el('BUTTON'))).toBe(true);
    expect(escMarksRead(null)).toBe(true);
  });
});
