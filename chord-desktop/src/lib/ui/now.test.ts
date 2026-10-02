import { describe, expect, it, vi } from 'vitest';
import { startClock, TICK_MS } from './now.svelte';

function fakeDoc() {
  let handler: () => void = () => {};
  return {
    hidden: false,
    addEventListener: (_: string, h: () => void) => (handler = h),
    removeEventListener: () => {},
    fire() {
      handler();
    }
  };
}

describe('startClock', () => {
  it('ticks while visible and stops while hidden', () => {
    vi.useFakeTimers();
    const doc = fakeDoc();
    const update = vi.fn();
    const stop = startClock(update, doc as never);
    vi.advanceTimersByTime(TICK_MS * 2);
    expect(update).toHaveBeenCalledTimes(2);
    doc.hidden = true;
    doc.fire();
    vi.advanceTimersByTime(TICK_MS * 10);
    expect(update).toHaveBeenCalledTimes(2);
    doc.hidden = false;
    doc.fire();
    expect(update).toHaveBeenCalledTimes(3);
    vi.advanceTimersByTime(TICK_MS);
    expect(update).toHaveBeenCalledTimes(4);
    stop();
    vi.advanceTimersByTime(TICK_MS * 3);
    expect(update).toHaveBeenCalledTimes(4);
    vi.useRealTimers();
  });
});
