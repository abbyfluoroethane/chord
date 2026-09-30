import { describe, expect, it } from 'vitest';
import { AVATAR_RETRY_MS, avatarTry } from './avatars';

describe('avatarTry', () => {
  const src = 'chord-avatar://localhost/bob%40example.org';

  it('keeps the URL for the first try', () => {
    expect(avatarTry(src, 0)).toBe(src);
  });

  it('gives a new URL for each new try', () => {
    const urls = new Set([0, 1, 2, 3].map((n) => avatarTry(src, n)));
    expect(urls.size).toBe(4);
    expect(avatarTry(src, 1).startsWith(`${src}?`)).toBe(true);
  });

  it('has a wait time for each new try', () => {
    expect(AVATAR_RETRY_MS.length).toBeGreaterThan(0);
  });
});
