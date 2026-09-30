import { describe, expect, it } from 'vitest';
import { dateOf, formatTimestamp, type TimeStyle } from './timestamp';

// 2025-01-01 00:00:00 UTC, on a Wednesday.
const SECONDS = 1735689600;
const opt = { locale: 'en-US', timeZone: 'UTC' };
const clean = (s: string) => s.replace(/[  ]/g, ' ');
const at = (style: TimeStyle, now = 0) =>
  clean(formatTimestamp(dateOf(SECONDS)!, style, now, opt));

describe('formatTimestamp', () => {
  it('formats the time styles', () => {
    expect(at('t')).toBe('12:00 AM');
    expect(at('T')).toBe('12:00:00 AM');
  });
  it('formats the date styles', () => {
    expect(at('d')).toBe('01/01/2025');
    expect(at('D')).toBe('January 1, 2025');
  });
  it('formats the long styles', () => {
    expect(at('f')).toBe('January 1, 2025 12:00 AM');
    expect(at('F')).toBe('Wednesday, January 1, 2025 12:00 AM');
  });
  it('formats a relative time', () => {
    const now = SECONDS * 1000;
    expect(at('R', now - 5 * 60_000)).toBe('in 5 minutes');
    expect(at('R', now + 3 * 86_400_000)).toBe('3 days ago');
    expect(at('R', now + 2 * 3_600_000)).toBe('2 hours ago');
    expect(at('R', now)).toBe('now');
    expect(at('R', now + 400 * 86_400_000)).toBe('last year');
  });
  it('uses the locale', () => {
    const fr = formatTimestamp(dateOf(SECONDS)!, 'D', 0, { locale: 'fr-FR', timeZone: 'UTC' });
    expect(fr).toBe('1 janvier 2025');
  });
});

describe('dateOf', () => {
  it('rejects a number that is not a date', () => {
    expect(dateOf(NaN)).toBeNull();
    expect(dateOf(1e15)).toBeNull();
    expect(dateOf(0)).not.toBeNull();
  });
});
