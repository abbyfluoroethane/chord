// The sound of a notification, and the rule for when it plays.
//
// The core already applied the level of each chat: it sends a `notification` event only for a
// message that may notify. This file adds the settings of the user on top. The beep comes from
// WebAudio, so the app needs no sound file.

/** The settings that the sound rule reads. */
export interface NoticeSettings {
  sound: boolean;
  muteDms: boolean;
}

/**
 * Should the notification play the sound? Not when the user reads that chat in a focused
 * window, and not for a chat when "mute DMs" is on. `room` is null for a chat and for a
 * private message from a room member.
 */
export function shouldChime(
  prefs: NoticeSettings,
  n: { room: string | null },
  looking: boolean
): boolean {
  if (!prefs.sound || looking) return false;
  return !(n.room === null && prefs.muteDms);
}

let context: AudioContext | null = null;

/** Play a short, quiet two-note beep. It fails without a sound: audio is never critical. */
export function beep(): void {
  try {
    context ??= new AudioContext();
    const ctx = context;
    if (ctx.state === 'suspended') void ctx.resume();
    const gain = ctx.createGain();
    gain.connect(ctx.destination);
    const start = ctx.currentTime;
    gain.gain.setValueAtTime(0.0001, start);
    gain.gain.exponentialRampToValueAtTime(0.06, start + 0.02);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + 0.3);
    for (const [i, hz] of [880, 1175].entries()) {
      const osc = ctx.createOscillator();
      osc.type = 'sine';
      osc.frequency.value = hz;
      osc.connect(gain);
      osc.start(start + i * 0.12);
      osc.stop(start + i * 0.12 + 0.16);
    }
  } catch {
    /* no audio device */
  }
}
