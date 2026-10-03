// The shape of the user prefs, and how to read them from saved data. No state here, so the
// rules are easy to test.
import type { DisplayMode } from './types';
import { EMOJI_PACK_IDS, type EmojiPackId } from './emojipackids';

export interface Saved {
  desktopNotifications: boolean;
  sound: boolean;
  muteDms: boolean;
  autoApprove: boolean;
  display: DisplayMode;
  fontSize: number;
  gifPicker: boolean;
  shareInfo: boolean;
  emojiPack: EmojiPackId;
  // Chat
  sendKey: SendKey;
  inlineMedia: boolean;
  autoplayVideo: boolean;
  showSpoilers: boolean;
  emoticons: boolean;
  spellcheck: boolean;
  confirmDelete: boolean;
  // Appearance
  timeFormat: TimeFormat;
  groupSpacing: GroupSpacing;
  jumboEmoji: boolean;
  animateGifs: AnimateGifs;
  /** The zoom of the whole app, in percent. */
  zoom: number;
  motion: MotionMode;
  showPresence: boolean;
  linkUnderline: LinkUnderline;
}

export type SendKey = 'enter' | 'mod-enter';

/** The Chat settings at first start. Each default keeps the behaviour from before the setting. */
export const CHAT_DEFAULTS = {
  sendKey: 'enter',
  inlineMedia: true,
  autoplayVideo: false,
  showSpoilers: false,
  emoticons: false,
  spellcheck: true,
  confirmDelete: true
} as const satisfies Pick<
  Saved,
  | 'sendKey'
  | 'inlineMedia'
  | 'autoplayVideo'
  | 'showSpoilers'
  | 'emoticons'
  | 'spellcheck'
  | 'confirmDelete'
>;
export type TimeFormat = 'system' | '12h' | '24h';
export type GroupSpacing = 'small' | 'normal' | 'large';
export type AnimateGifs = 'always' | 'hover' | 'never';
/** `system` follows the device. `reduce` always cuts motion. `full` never cuts it. */
export type MotionMode = 'system' | 'reduce' | 'full';
export type LinkUnderline = 'always' | 'hover';

/** The Intl hour cycle for a time format. System gives none, so the locale decides. */
export function hourCycleOf(f: TimeFormat): 'h12' | 'h23' | undefined {
  return f === '12h' ? 'h12' : f === '24h' ? 'h23' : undefined;
}

export const ZOOM_MIN = 80;
export const ZOOM_MAX = 150;

/** The key in the settings file, and the old localStorage key of the same blob. */
export const SETTINGS_KEY = 'prefs';
export const LEGACY_KEY = 'chord.prefs';

/** Keep the fields that have the right type. Anything else is left out. */
export function parsePrefs(v: unknown): Partial<Saved> {
  const out: Partial<Saved> = {};
  if (!v || typeof v !== 'object') return out;
  const o = v as Record<string, unknown>;
  if (typeof o.desktopNotifications === 'boolean') out.desktopNotifications = o.desktopNotifications;
  if (typeof o.sound === 'boolean') out.sound = o.sound;
  if (typeof o.muteDms === 'boolean') out.muteDms = o.muteDms;
  if (typeof o.autoApprove === 'boolean') out.autoApprove = o.autoApprove;
  if (o.display === 'cozy' || o.display === 'compact') out.display = o.display;
  if (typeof o.gifPicker === 'boolean') out.gifPicker = o.gifPicker;
  if (typeof o.shareInfo === 'boolean') out.shareInfo = o.shareInfo;
  if (typeof o.emojiPack === 'string' && (EMOJI_PACK_IDS as readonly string[]).includes(o.emojiPack))
    out.emojiPack = o.emojiPack as EmojiPackId;
  // Chat
  if (o.sendKey === 'enter' || o.sendKey === 'mod-enter') out.sendKey = o.sendKey;
  if (typeof o.inlineMedia === 'boolean') out.inlineMedia = o.inlineMedia;
  if (typeof o.autoplayVideo === 'boolean') out.autoplayVideo = o.autoplayVideo;
  if (typeof o.showSpoilers === 'boolean') out.showSpoilers = o.showSpoilers;
  if (typeof o.emoticons === 'boolean') out.emoticons = o.emoticons;
  if (typeof o.spellcheck === 'boolean') out.spellcheck = o.spellcheck;
  if (typeof o.confirmDelete === 'boolean') out.confirmDelete = o.confirmDelete;
  if (typeof o.fontSize === 'number' && Number.isFinite(o.fontSize))
    out.fontSize = Math.min(20, Math.max(12, o.fontSize));
  // Appearance
  if (o.timeFormat === 'system' || o.timeFormat === '12h' || o.timeFormat === '24h')
    out.timeFormat = o.timeFormat;
  if (o.groupSpacing === 'small' || o.groupSpacing === 'normal' || o.groupSpacing === 'large')
    out.groupSpacing = o.groupSpacing;
  if (typeof o.jumboEmoji === 'boolean') out.jumboEmoji = o.jumboEmoji;
  if (o.animateGifs === 'always' || o.animateGifs === 'hover' || o.animateGifs === 'never')
    out.animateGifs = o.animateGifs;
  if (typeof o.zoom === 'number' && Number.isFinite(o.zoom))
    out.zoom = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(o.zoom)));
  if (o.motion === 'system' || o.motion === 'reduce' || o.motion === 'full') out.motion = o.motion;
  if (typeof o.showPresence === 'boolean') out.showPresence = o.showPresence;
  if (o.linkUnderline === 'always' || o.linkUnderline === 'hover') out.linkUnderline = o.linkUnderline;
  return out;
}

/** Parse the JSON text of the old localStorage value. Bad text gives no prefs. */
export function parseLegacy(raw: string | null): Partial<Saved> {
  if (!raw) return {};
  try {
    return parsePrefs(JSON.parse(raw));
  } catch {
    return {};
  }
}

/**
 * The prefs to use at start. The settings file wins. A field that the file does not have
 * comes from the old localStorage copy (one version of fallback), and `migrate` is true when
 * the old copy gave at least one field, so the caller writes the file. Nothing is deleted.
 */
export function resolvePrefs(
  stored: unknown,
  legacyRaw: string | null
): { prefs: Partial<Saved>; migrate: boolean } {
  const file = parsePrefs(stored);
  const legacy = parseLegacy(legacyRaw);
  const extra = Object.keys(legacy).filter((k) => !(k in file));
  return { prefs: { ...legacy, ...file }, migrate: extra.length > 0 };
}
