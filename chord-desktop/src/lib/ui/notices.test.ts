import { describe, expect, it } from 'vitest';
import { shouldChime } from './notices';

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
