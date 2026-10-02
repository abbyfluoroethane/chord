import { describe, expect, it } from 'vitest';
import { bindings, comboLabel, findBinding, matches, neighbor, railOrder, type KeyLike } from './keymap';

function press(key: string, code: string, mods: Partial<KeyLike> = {}): KeyLike {
  return { key, code, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...mods };
}

const find = (e: KeyLike, mac: boolean) => findBinding(e, mac)?.action ?? null;

describe('the mod key', () => {
  it('is Cmd on a Mac and Ctrl elsewhere', () => {
    expect(find(press('e', 'KeyE', { metaKey: true }), true)).toBe('emoji');
    expect(find(press('e', 'KeyE', { ctrlKey: true }), true)).toBeNull();
    expect(find(press('e', 'KeyE', { ctrlKey: true }), false)).toBe('emoji');
    expect(find(press('e', 'KeyE', { metaKey: true }), false)).toBeNull();
  });

  it('needs the exact modifiers', () => {
    expect(find(press('e', 'KeyE'), false)).toBeNull();
    expect(find(press('e', 'KeyE', { ctrlKey: true, shiftKey: true }), false)).toBeNull();
    expect(find(press('e', 'KeyE', { ctrlKey: true, altKey: true }), false)).toBeNull();
  });

  it('keeps Shift apart for a letter', () => {
    expect(find(press('u', 'KeyU', { ctrlKey: true }), false)).toBe('members');
    expect(find(press('U', 'KeyU', { ctrlKey: true, shiftKey: true }), false)).toBe('upload');
  });
});

describe('layouts', () => {
  it('matches a Cyrillic layout by the physical key', () => {
    expect(find(press('у', 'KeyE', { ctrlKey: true }), false)).toBe('emoji');
    expect(find(press('б', 'Comma', { metaKey: true }), true)).toBe('settings');
  });

  it('matches a Dvorak layout by the letter typed', () => {
    // The key in the QWERTY E place types a period on Dvorak.
    expect(find(press('.', 'KeyE', { ctrlKey: true }), false)).toBeNull();
    // The letter E sits in the QWERTY D place.
    expect(find(press('e', 'KeyD', { ctrlKey: true }), false)).toBe('emoji');
  });

  it('accepts a symbol that needs Shift on the layout', () => {
    expect(find(press('/', 'Digit7', { ctrlKey: true, shiftKey: true }), false)).toBe('shortcuts');
  });

  it('uses the physical key when Option changes the letter on a Mac', () => {
    expect(find(press('®', 'KeyR', { altKey: true }), true)).toBe('reply');
    expect(find(press('r', 'KeyR', { altKey: true }), false)).toBe('reply');
  });

  it('does not treat AltGr text as a shortcut', () => {
    // On Windows, AltGr reports Ctrl and Alt together.
    expect(find(press('€', 'KeyE', { ctrlKey: true, altKey: true }), false)).toBeNull();
  });
});

describe('named keys', () => {
  it('tells the channel, unread, and space steps apart', () => {
    expect(find(press('ArrowUp', 'ArrowUp', { altKey: true }), true)).toBe('channelPrev');
    expect(find(press('ArrowDown', 'ArrowDown', { altKey: true, shiftKey: true }), true)).toBe('unreadNext');
    expect(find(press('ArrowUp', 'ArrowUp', { altKey: true, metaKey: true }), true)).toBe('spacePrev');
    expect(find(press('ArrowDown', 'ArrowDown', { altKey: true, ctrlKey: true }), false)).toBe('spaceNext');
  });

  it('has Ctrl+Tab as an alternate for the space step on every system', () => {
    expect(find(press('Tab', 'Tab', { ctrlKey: true }), true)).toBe('spaceNext');
    expect(find(press('Tab', 'Tab', { ctrlKey: true, shiftKey: true }), false)).toBe('spacePrev');
    expect(find(press('Tab', 'Tab'), false)).toBeNull();
  });

  it('separates the page keys by Shift', () => {
    expect(find(press('PageUp', 'PageUp'), false)).toBe('pageUp');
    expect(find(press('PageUp', 'PageUp', { shiftKey: true }), false)).toBe('oldestUnread');
    expect(find(press('PageDown', 'PageDown', { shiftKey: true }), false)).toBe('present');
    expect(find(press('Escape', 'Escape'), false)).toBe('markRead');
    expect(find(press('Escape', 'Escape', { shiftKey: true }), false)).toBeNull();
  });
});

describe('the list', () => {
  const RESERVED = ['q', 'w', 'c', 'v', 'x', 'z', 'a', 'r', 'n', 't', 'h', 'm', 's', 'l', 'o'];

  it('takes no key that the system or the webview uses', () => {
    for (const b of bindings) {
      for (const c of b.combos) {
        if (c.mod && !c.shift && !c.alt) expect(RESERVED).not.toContain(c.key);
        if (c.mod && c.shift && c.key === 'r') throw new Error('reload');
      }
    }
  });

  it('has no two actions on the same key', () => {
    for (const mac of [true, false]) {
      const seen = new Map<string, string>();
      for (const b of bindings) {
        for (const c of b.combos) {
          const id = [
            c.mod && mac ? 'meta' : '',
            c.ctrl || (c.mod && !mac) ? 'ctrl' : '',
            c.alt ? 'alt' : '',
            c.shift ? 'shift' : '',
            c.key
          ].join('+');
          expect(seen.get(id), `${id} on ${mac ? 'mac' : 'pc'}`).toBeUndefined();
          seen.set(id, b.action);
        }
      }
    }
  });

  it('finds each combo through its own label', () => {
    for (const b of bindings) {
      for (const c of b.combos) {
        const e = press(c.key, '', {
          metaKey: !!c.mod,
          ctrlKey: !!c.ctrl,
          altKey: !!c.alt,
          shiftKey: !!c.shift
        });
        expect(matches(e, c, true)).toBe(true);
        expect(comboLabel(c, true).length).toBeGreaterThan(0);
      }
    }
  });

  it('writes the labels for each system', () => {
    const emoji = bindings.find((b) => b.action === 'emoji')!.combos[0];
    expect(comboLabel(emoji, true)).toEqual(['Cmd', 'E']);
    expect(comboLabel(emoji, false)).toEqual(['Ctrl', 'E']);
    const reply = bindings.find((b) => b.action === 'reply')!.combos[0];
    expect(comboLabel(reply, true)).toEqual(['Option', 'R']);
    expect(comboLabel(reply, false)).toEqual(['Alt', 'R']);
  });
});

describe('space order', () => {
  it('wraps around the list', () => {
    expect(neighbor(['a', 'b', 'c'], 'c', 1)).toBe('a');
    expect(neighbor(['a', 'b', 'c'], 'a', -1)).toBe('c');
    expect(neighbor([], 'a', 1)).toBeNull();
  });

  it('starts at the first or last item when the current one is unknown', () => {
    expect(neighbor(['a', 'b'], 'x', 1)).toBe('a');
    expect(neighbor(['a', 'b'], 'x', -1)).toBe('b');
  });

  it('follows the rail, opens folders, and drops unknown spaces', () => {
    const layout = [
      { kind: 'circle' as const, id: 'b' },
      { kind: 'folder' as const, circles: ['gone', 'a'] }
    ];
    expect(railOrder(layout, ['a', 'b', 'c'])).toEqual(['b', 'a', 'c']);
  });
});
