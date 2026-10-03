import { describe, expect, it } from 'vitest';
import { mayMove } from './still.svelte';

describe('mayMove', () => {
  it('accepts the types that can animate', () => {
    expect(mayMove('image/gif')).toBe(true);
    expect(mayMove('image/webp')).toBe(true);
    expect(mayMove('data:image/gif;base64,R0lG')).toBe(true);
  });
  it('rejects the other types', () => {
    expect(mayMove('image/png')).toBe(false);
    expect(mayMove('data:image/jpeg;base64,/9j')).toBe(false);
    expect(mayMove(null)).toBe(false);
  });
});
