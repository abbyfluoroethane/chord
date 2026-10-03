// The sound of a notification, and the rule for when it plays.
//
// The core already applied the level of each chat: it sends a `notification` event only for a
// message that may notify. This file adds the settings of the user on top. The sounds come from
// WebAudio, so the app needs no sound file.
import type { SoundId } from './prefsdata';

/** The settings that the sound rule reads. */
export interface NoticeSettings {
  sound: boolean;
  muteDms: boolean;
  quietHours?: boolean;
  quietFrom?: number;
  quietTo?: number;
}

/**
 * Is `now` inside the quiet window? All three are minutes after midnight. The window can
 * cross midnight, as 22:00 to 08:00. Equal times give an empty window.
 */
export function inQuietHours(from: number, to: number, now: number): boolean {
  if (from === to) return false;
  return from < to ? now >= from && now < to : now >= from || now < to;
}

/** Minutes after midnight in local time. */
export function minutesNow(d: Date = new Date()): number {
  return d.getHours() * 60 + d.getMinutes();
}

/** The text of a time input ("07:30") for minutes after midnight. */
export function toTimeText(minutes: number): string {
  const h = Math.floor(minutes / 60);
  return `${String(h).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
}

/** Minutes after midnight for the text of a time input. Bad text gives null. */
export function fromTimeText(text: string): number | null {
  const m = /^(\d{2}):(\d{2})$/.exec(text);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  return h < 24 && min < 60 ? h * 60 + min : null;
}

/**
 * Should the notification play the sound? Not when the user reads that chat in a focused
 * window, not in the quiet hours, and not for a chat when "mute DMs" is on. `room` is null for
 * a chat and for a private message from a room member.
 */
export function shouldChime(
  prefs: NoticeSettings,
  n: { room: string | null },
  looking: boolean,
  now: number = minutesNow()
): boolean {
  if (!prefs.sound || looking) return false;
  if (prefs.quietHours && inQuietHours(prefs.quietFrom ?? 0, prefs.quietTo ?? 0, now)) return false;
  return !(n.room === null && prefs.muteDms);
}

/** The names of the sounds, for the picker. */
export const SOUNDS: { value: SoundId; label: string }[] = [
  { value: 'chime', label: 'Chime' },
  { value: 'pop', label: 'Pop' },
  { value: 'ping', label: 'Ping' },
  { value: 'drop', label: 'Drop' }
];

/** One note of a sound: the pitch, the start, the length, and the wave. */
interface Note {
  hz: number;
  at: number;
  len: number;
  type: OscillatorType;
}

/** Each sound is a few notes. The peak gain of the whole sound comes from the volume. */
const NOTES: Record<SoundId, Note[]> = {
  chime: [
    { hz: 880, at: 0, len: 0.16, type: 'sine' },
    { hz: 1175, at: 0.12, len: 0.16, type: 'sine' }
  ],
  pop: [{ hz: 520, at: 0, len: 0.09, type: 'triangle' }],
  ping: [
    { hz: 1568, at: 0, len: 0.4, type: 'sine' },
    { hz: 2349, at: 0, len: 0.25, type: 'sine' }
  ],
  drop: [
    { hz: 659, at: 0, len: 0.12, type: 'sine' },
    { hz: 523, at: 0.1, len: 0.12, type: 'sine' },
    { hz: 392, at: 0.2, len: 0.16, type: 'sine' }
  ]
};

/** The peak gain for a volume from 0 to 100. 50 gives 0.06, the level of the old beep. */
export function gainFor(volume: number): number {
  return (Math.min(100, Math.max(0, volume)) / 100) * 0.12;
}

let context: AudioContext | null = null;

/** Play a short sound. It fails without a sound: audio is never critical. */
export function beep(sound: SoundId = 'chime', volume = 50): void {
  const peak = gainFor(volume);
  if (peak <= 0) return;
  try {
    context ??= new AudioContext();
    const ctx = context;
    if (ctx.state === 'suspended') void ctx.resume();
    const gain = ctx.createGain();
    gain.connect(ctx.destination);
    const start = ctx.currentTime;
    const notes = NOTES[sound] ?? NOTES.chime;
    const end = Math.max(...notes.map((n) => n.at + n.len)) + 0.14;
    gain.gain.setValueAtTime(0.0001, start);
    gain.gain.exponentialRampToValueAtTime(peak, start + 0.02);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + end);
    for (const n of notes) {
      const osc = ctx.createOscillator();
      osc.type = n.type;
      osc.frequency.value = n.hz;
      osc.connect(gain);
      osc.start(start + n.at);
      osc.stop(start + n.at + n.len + 0.1);
    }
  } catch {
    /* no audio device */
  }
}
