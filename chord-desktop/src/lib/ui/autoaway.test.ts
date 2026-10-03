import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AutoAway, POLL_MS, watchAutoAway, type AwayEnv } from './autoaway';
import type { Show } from './types';

function fakeIO(start: Show) {
  const io = {
    current: start,
    show: () => io.current,
    setShow: vi.fn((s: Show) => {
      io.current = s;
    })
  };
  return io;
}

const FIVE = 5 * 60_000;

describe('AutoAway', () => {
  it('sets Away after the idle time and brings the status back', () => {
    const io = fakeIO('chat');
    const a = new AutoAway(io, FIVE);
    a.tick(FIVE - 1);
    expect(io.setShow).not.toHaveBeenCalled();
    a.tick(FIVE);
    expect(io.current).toBe('away');
    a.tick(FIVE + 60_000);
    expect(io.setShow).toHaveBeenCalledTimes(1);
    a.tick(1000);
    expect(io.current).toBe('chat');
  });

  it('treats a null status as online and restores null', () => {
    const io = fakeIO(null);
    const a = new AutoAway(io, FIVE);
    a.tick(FIVE);
    expect(io.current).toBe('away');
    a.back();
    expect(io.current).toBeNull();
  });

  it('does not touch Away, Do not disturb or Invisible', () => {
    for (const s of ['away', 'xa', 'dnd', 'invisible'] as const) {
      const io = fakeIO(s);
      const a = new AutoAway(io, FIVE);
      a.tick(FIVE * 3);
      a.tick(0);
      expect(io.setShow).not.toHaveBeenCalled();
      expect(io.current).toBe(s);
    }
  });

  it('leaves a status that the user picks during auto-away', () => {
    const io = fakeIO('chat');
    const a = new AutoAway(io, FIVE);
    a.tick(FIVE);
    io.current = 'dnd';
    a.tick(FIVE + 1000);
    a.tick(0);
    expect(io.current).toBe('dnd');
    expect(io.setShow).toHaveBeenCalledTimes(1);
  });

  it('does not set Away again until the user was active', () => {
    const io = fakeIO('chat');
    const a = new AutoAway(io, FIVE);
    a.tick(FIVE);
    io.current = 'chat';
    a.tick(FIVE + 1000);
    expect(io.setShow).toHaveBeenCalledTimes(1);
    a.tick(0);
    a.tick(FIVE);
    expect(io.setShow).toHaveBeenCalledTimes(2);
  });
});

describe('watchAutoAway', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function env(sys: () => number | null) {
    let input: () => void = () => {};
    const e: AwayEnv = {
      now: () => Date.now(),
      listen(h) {
        input = h;
        return () => {
          input = () => {};
        };
      },
      setTimeout: (h, ms) => setTimeout(h, ms),
      clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
      systemIdle: async () => sys()
    };
    return { e, input: () => input() };
  }

  it('uses the system idle time when the OS gives it', async () => {
    const io = fakeIO('chat');
    let idle = 0;
    const { e } = env(() => idle);
    watchAutoAway(io, 5, e);
    await vi.advanceTimersByTimeAsync(0);
    idle = 301;
    await vi.advanceTimersByTimeAsync(POLL_MS);
    expect(io.current).toBe('away');
    idle = 2;
    await vi.advanceTimersByTimeAsync(POLL_MS);
    expect(io.current).toBe('chat');
  });

  it('counts window input when the OS gives no time, and a click ends it at once', async () => {
    const io = fakeIO('chat');
    const { e, input } = env(() => null);
    watchAutoAway(io, 5, e);
    await vi.advanceTimersByTimeAsync(FIVE + POLL_MS);
    expect(io.current).toBe('away');
    input();
    expect(io.current).toBe('chat');
  });

  it('stop brings the status back and ends the polling', async () => {
    const io = fakeIO('chat');
    const { e } = env(() => 9999);
    const stop = watchAutoAway(io, 5, e);
    await vi.advanceTimersByTimeAsync(0);
    expect(io.current).toBe('away');
    stop();
    expect(io.current).toBe('chat');
    await vi.advanceTimersByTimeAsync(POLL_MS * 3);
    expect(io.setShow).toHaveBeenCalledTimes(2);
  });
});
