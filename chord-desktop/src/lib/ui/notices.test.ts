import { describe, expect, it } from 'vitest';
import { fromTimeText, gainFor, inQuietHours, shouldChime, toTimeText } from './notices';

const on = { sound: true, muteDms: false };

describe('shouldChime', () => {
  it('plays for a message in a chat that the user does not read', () => {
    expect(shouldChime(on, { room: null }, false)).toBe(true);
    expect(shouldChime(on, { room: 'dev@muc.example.org' }, false)).toBe(true);
  });

  it('stays silent when the sound is off', () => {
    expect(shouldChime({ ...on, sound: false }, { room: null }, false)).toBe(false);
  });

  it('stays silent for the chat that the user reads', () => {
    expect(shouldChime(on, { room: null }, true)).toBe(false);
  });

  it('mutes chats but not rooms with "mute DMs"', () => {
    const muted = { sound: true, muteDms: true };
    expect(shouldChime(muted, { room: null }, false)).toBe(false);
    expect(shouldChime(muted, { room: 'dev@muc.example.org' }, false)).toBe(true);
  });
});

describe('inQuietHours', () => {
  it('covers a window inside one day', () => {
    expect(inQuietHours(60, 120, 60)).toBe(true);
    expect(inQuietHours(60, 120, 119)).toBe(true);
    expect(inQuietHours(60, 120, 120)).toBe(false);
    expect(inQuietHours(60, 120, 30)).toBe(false);
  });
  it('covers a window that crosses midnight', () => {
    expect(inQuietHours(1320, 480, 1380)).toBe(true);
    expect(inQuietHours(1320, 480, 0)).toBe(true);
    expect(inQuietHours(1320, 480, 479)).toBe(true);
    expect(inQuietHours(1320, 480, 480)).toBe(false);
    expect(inQuietHours(1320, 480, 720)).toBe(false);
  });
  it('gives an empty window for equal times', () => {
    expect(inQuietHours(300, 300, 300)).toBe(false);
  });
});

describe('quiet hours and the sound', () => {
  const quiet = { sound: true, muteDms: false, quietHours: true, quietFrom: 1320, quietTo: 480 };
  it('stays silent inside the window', () => {
    expect(shouldChime(quiet, { room: null }, false, 1400)).toBe(false);
  });
  it('plays outside the window', () => {
    expect(shouldChime(quiet, { room: null }, false, 600)).toBe(true);
  });
  it('ignores the window when the switch is off', () => {
    expect(shouldChime({ ...quiet, quietHours: false }, { room: null }, false, 1400)).toBe(true);
  });
});

describe('time text', () => {
  it('converts both ways', () => {
    expect(toTimeText(1320)).toBe('22:00');
    expect(toTimeText(65)).toBe('01:05');
    expect(fromTimeText('07:30')).toBe(450);
    expect(fromTimeText('24:00')).toBeNull();
    expect(fromTimeText('')).toBeNull();
    expect(fromTimeText('7:3')).toBeNull();
  });
});

describe('gainFor', () => {
  it('keeps the old level at 50 and clamps the ends', () => {
    expect(gainFor(50)).toBeCloseTo(0.06);
    expect(gainFor(0)).toBe(0);
    expect(gainFor(900)).toBeCloseTo(0.12);
    expect(gainFor(-3)).toBe(0);
  });
});
