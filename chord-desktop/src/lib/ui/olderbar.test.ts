import { describe, expect, it } from 'vitest';
import { messagesBelow, OLDER_BAR_MESSAGES, showOlderBar } from './olderbar';

// 100 messages, each 50 px high, the first at 0.
const top = (i: number) => i * 50;

describe('messagesBelow', () => {
  it('counts the messages that start at or below the bottom edge', () => {
    expect(messagesBelow(100, top, 5000)).toBe(0);
    expect(messagesBelow(100, top, 4950)).toBe(1);
    expect(messagesBelow(100, top, 4951)).toBe(0);
    expect(messagesBelow(100, top, 2500)).toBe(50);
    expect(messagesBelow(100, top, 0)).toBe(100);
  });
  it('handles an empty list', () => {
    expect(messagesBelow(0, top, 100)).toBe(0);
  });
  it('does not count a message that is partly on screen', () => {
    expect(messagesBelow(100, top, 2520)).toBe(49);
  });
});

describe('showOlderBar', () => {
  it('stays hidden for a small scroll up', () => {
    expect(showOlderBar(0)).toBe(false);
    expect(showOlderBar(OLDER_BAR_MESSAGES - 1)).toBe(false);
  });
  it('shows at the limit and above', () => {
    expect(showOlderBar(OLDER_BAR_MESSAGES)).toBe(true);
    expect(showOlderBar(500)).toBe(true);
  });
});
