import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IDLE_AFTER, idleLabel, watchIdle, type Env } from './idle';

function fakeEnv() {
  let handler: () => void = () => {};
  const api: Env = {
    now: () => Date.now(),
    listen(h) {
      handler = h;
      return () => {
        handler = () => {};
      };
    },
    setTimeout: (h, ms) => setTimeout(h, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>)
  };
  return { api, input: () => handler() };
}

describe('watchIdle', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('sends nothing while the user is active', () => {
    const { api, input } = fakeEnv();
    const send = vi.fn();
    watchIdle(send, api);
    vi.advanceTimersByTime(IDLE_AFTER - 1000);
    input();
    vi.advanceTimersByTime(IDLE_AFTER - 1000);
    expect(send).not.toHaveBeenCalled();
  });

  it('sends the time of the last input when the wait ends, and null on the next input', () => {
    vi.setSystemTime(1_000_000);
    const { api, input } = fakeEnv();
    const send = vi.fn();
    watchIdle(send, api);
    vi.advanceTimersByTime(60_000);
    input();
    const lastInput = Date.now();
    vi.advanceTimersByTime(IDLE_AFTER);
    expect(send.mock.calls).toEqual([[lastInput]]);
    input();
    expect(send.mock.calls).toEqual([[lastInput], [null]]);
  });

  it('resends only while idle, and stops', () => {
    const { api, input } = fakeEnv();
    const send = vi.fn();
    const watch = watchIdle(send, api);
    watch.resend();
    expect(send).not.toHaveBeenCalled();
    vi.advanceTimersByTime(IDLE_AFTER);
    watch.resend();
    expect(send).toHaveBeenCalledTimes(2);
    watch.stop();
    input();
    vi.advanceTimersByTime(IDLE_AFTER * 2);
    expect(send).toHaveBeenCalledTimes(2);
  });
});

describe('idleLabel', () => {
  const now = Date.parse('2026-09-30T12:00:00Z');
  it('words the time', () => {
    expect(idleLabel(null, now)).toBeNull();
    expect(idleLabel('bad', now)).toBeNull();
    expect(idleLabel('2026-09-30T11:59:40Z', now)).toBe('Idle');
    expect(idleLabel('2026-09-30T11:48:00Z', now)).toBe('Idle 12 min');
    expect(idleLabel('2026-09-30T09:00:00Z', now)).toBe('Idle 3 h');
    expect(idleLabel('2026-09-26T12:00:00Z', now)).toBe('Idle 4 d');
  });
});
