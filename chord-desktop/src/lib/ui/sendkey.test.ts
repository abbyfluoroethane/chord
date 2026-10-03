import { describe, expect, it } from 'vitest';
import { isSendKey } from './sendkey';

const key = (o: Partial<Parameters<typeof isSendKey>[0]> = {}) => ({
  key: 'Enter',
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  isComposing: false,
  ...o
});

describe('isSendKey', () => {
  it('sends on Enter and adds a line on Shift+Enter', () => {
    expect(isSendKey(key(), 'enter')).toBe(true);
    expect(isSendKey(key({ shiftKey: true }), 'enter')).toBe(false);
  });
  it('sends on Ctrl+Enter or Cmd+Enter only, in the other mode', () => {
    expect(isSendKey(key(), 'mod-enter')).toBe(false);
    expect(isSendKey(key({ ctrlKey: true }), 'mod-enter')).toBe(true);
    expect(isSendKey(key({ metaKey: true }), 'mod-enter')).toBe(true);
  });
  it('ignores other keys and text composition', () => {
    expect(isSendKey(key({ key: 'a' }), 'enter')).toBe(false);
    expect(isSendKey(key({ isComposing: true }), 'enter')).toBe(false);
  });
});
