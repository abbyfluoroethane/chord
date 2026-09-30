import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { BLUR_DELAY, watchClientState, type Env } from './clientstate';

/** A stand-in for the page: set `visible` and `focused`, then fire an event. */
function fakeEnv() {
  const handlers: Record<string, Array<() => void>> = {};
  const env = {
    visible: true,
    focused: true,
    fire(event: string) {
      handlers[event]?.forEach((h) => h());
    }
  };
  const api: Env = {
    visible: () => env.visible,
    focused: () => env.focused,
    listen(event, handler) {
      (handlers[event] ??= []).push(handler);
      return () => {
        handlers[event] = handlers[event].filter((h) => h !== handler);
      };
    },
    setTimeout: (handler, ms) => setTimeout(handler, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>)
  };
  return { env, api };
}

describe('watchClientState', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('sends the first state at once', () => {
    const { api } = fakeEnv();
    const send = vi.fn();
    watchClientState(send, api);
    expect(send.mock.calls).toEqual([[true]]);
  });

  it('goes inactive at once when the page is hidden, and active when it shows', () => {
    const { env, api } = fakeEnv();
    const send = vi.fn();
    watchClientState(send, api);
    env.visible = false;
    env.fire('visibilitychange');
    env.visible = true;
    env.fire('visibilitychange');
    expect(send.mock.calls).toEqual([[true], [false], [true]]);
  });

  it('waits before a lost focus counts, and a return in time sends nothing', () => {
    const { env, api } = fakeEnv();
    const send = vi.fn();
    watchClientState(send, api);
    env.focused = false;
    env.fire('blur');
    vi.advanceTimersByTime(BLUR_DELAY - 1);
    expect(send.mock.calls).toEqual([[true]]);
    env.focused = true;
    env.fire('focus');
    vi.advanceTimersByTime(BLUR_DELAY * 2);
    expect(send.mock.calls).toEqual([[true]]);
  });

  it('goes inactive after the delay, and active on focus', () => {
    const { env, api } = fakeEnv();
    const send = vi.fn();
    watchClientState(send, api);
    env.focused = false;
    env.fire('blur');
    vi.advanceTimersByTime(BLUR_DELAY);
    expect(send.mock.calls).toEqual([[true], [false]]);
    env.focused = true;
    env.fire('focus');
    expect(send.mock.calls).toEqual([[true], [false], [true]]);
  });

  it('stops on cleanup', () => {
    const { env, api } = fakeEnv();
    const send = vi.fn();
    const stop = watchClientState(send, api);
    env.focused = false;
    env.fire('blur');
    stop();
    vi.advanceTimersByTime(BLUR_DELAY * 2);
    env.visible = false;
    env.fire('visibilitychange');
    expect(send.mock.calls).toEqual([[true]]);
  });
});
