// User settings. Inside the app they live in the settings file with the rest of the local
// data (get_settings and set_settings), under the key "prefs". Rust reads the two notice
// switches from there. In the browser preview they stay in localStorage.
//
// Migration: the first start after the update copies the old localStorage value into the
// file. The old key stays, and is read as a fallback for the fields the file lacks.
import type { DisplayMode } from './types';
import type { EmojiPackId } from './emojipackids';
import { live } from './bridge';
import { settings } from './local';
import {
  LEGACY_KEY,
  SETTINGS_KEY,
  CHAT_DEFAULTS,
  resolvePrefs,
  type AnimateGifs,
  type GroupSpacing,
  type LinkUnderline,
  type MotionMode,
  type Saved,
  type SendKey,
  type TimeFormat,
  type SoundId
} from './prefsdata';

const GROUP_GAP = { small: '8px', normal: '20px', large: '32px' } as const;

/** The webview zooms the whole page, so popups and scroll stay exact. The preview uses CSS zoom. */
async function applyZoom(percent: number) {
  const factor = percent / 100;
  if (live) {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      await getCurrentWebview().setZoom(factor);
    } catch {
      /* no zoom: the page keeps its size */
    }
    return;
  }
  document.documentElement.style.zoom = factor === 1 ? '' : String(factor);
}

class Prefs {
  desktopNotifications = $state(true);
  sound = $state(true);
  muteDms = $state(false);
  autoApprove = $state(false);
  display = $state<DisplayMode>('cozy');
  fontSize = $state(15);
  /** The GIF picker sends the search text to KLIPY. On by default, off in Privacy. */
  gifPicker = $state(true);
  /** Chord answers version and time queries. On by default, off in Privacy. Rust reads it at open. */
  shareInfo = $state(true);
  /** The images for emoji. Twemoji ships with the app. */
  emojiPack = $state<EmojiPackId>('twemoji');
  // Appearance. The defaults keep the look of the first version.
  timeFormat = $state<TimeFormat>('24h');
  groupSpacing = $state<GroupSpacing>('normal');
  jumboEmoji = $state(true);
  animateGifs = $state<AnimateGifs>('always');
  zoom = $state(100);
  motion = $state<MotionMode>('system');
  showPresence = $state(true);
  linkUnderline = $state<LinkUnderline>('always');

  // Chat
  sendKey = $state<SendKey>(CHAT_DEFAULTS.sendKey);
  inlineMedia = $state<boolean>(CHAT_DEFAULTS.inlineMedia);
  autoplayVideo = $state<boolean>(CHAT_DEFAULTS.autoplayVideo);
  showSpoilers = $state<boolean>(CHAT_DEFAULTS.showSpoilers);
  emoticons = $state<boolean>(CHAT_DEFAULTS.emoticons);
  spellcheck = $state<boolean>(CHAT_DEFAULTS.spellcheck);
  confirmDelete = $state<boolean>(CHAT_DEFAULTS.confirmDelete);

  // Notifications
  /** The system notice shows the text of the message. Rust reads it. */
  noticePreview = $state(true);
  /** No notice and no sound between two times. The times are minutes after midnight. */
  quietHours = $state(false);
  quietFrom = $state(22 * 60);
  quietTo = $state(8 * 60);
  soundChoice = $state<SoundId>('chime');
  /** 0 to 100. 50 is the level of the old fixed beep. */
  soundVolume = $state(50);
  /** The unread count in the window title and on the dock icon. */
  unreadBadge = $state(true);

  load() {
    let legacy: string | null = null;
    try {
      legacy = localStorage.getItem(LEGACY_KEY);
    } catch {
      /* storage blocked, keep the defaults */
    }
    const { prefs: v, migrate } = resolvePrefs(live ? settings.get(SETTINGS_KEY) : undefined, legacy);
    Object.assign(this, v);
    // Live, the file is the home. A migrated value goes there now.
    if (live && migrate) this.save();
    this.applyFont();
    this.applyLook();
    if (this.zoom !== 100) void applyZoom(this.zoom);
  }

  /** Change one setting, save all, and apply the font size. */
  set<K extends keyof Saved>(key: K, value: Saved[K]) {
    (this as unknown as Saved)[key] = value;
    this.save();
    if (key === 'fontSize') this.applyFont();
    this.applyLook();
    if (key === 'zoom') void applyZoom(this.zoom);
  }

  private snapshot(): Saved {
    return {
      desktopNotifications: this.desktopNotifications,
      sound: this.sound,
      muteDms: this.muteDms,
      autoApprove: this.autoApprove,
      display: this.display,
      fontSize: this.fontSize,
      gifPicker: this.gifPicker,
      shareInfo: this.shareInfo,
      emojiPack: this.emojiPack,
      // Chat
      sendKey: this.sendKey,
      inlineMedia: this.inlineMedia,
      autoplayVideo: this.autoplayVideo,
      showSpoilers: this.showSpoilers,
      emoticons: this.emoticons,
      spellcheck: this.spellcheck,
      confirmDelete: this.confirmDelete,
      // Appearance
      timeFormat: this.timeFormat,
      groupSpacing: this.groupSpacing,
      jumboEmoji: this.jumboEmoji,
      animateGifs: this.animateGifs,
      zoom: this.zoom,
      motion: this.motion,
      showPresence: this.showPresence,
      linkUnderline: this.linkUnderline,
      noticePreview: this.noticePreview,
      quietHours: this.quietHours,
      quietFrom: this.quietFrom,
      quietTo: this.quietTo,
      soundChoice: this.soundChoice,
      soundVolume: this.soundVolume,
      unreadBadge: this.unreadBadge
    };
  }

  private save() {
    const out = this.snapshot();
    if (live) {
      settings.set(SETTINGS_KEY, out);
      return;
    }
    try {
      localStorage.setItem(LEGACY_KEY, JSON.stringify(out));
    } catch {
      /* ignore */
    }
  }

  /** True when motion must be cut: the choice, or the device when the choice is system. */
  get reduceMotion(): boolean {
    if (this.motion !== 'system') return this.motion === 'reduce';
    return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;
  }

  /** Put the look prefs on the root element. The style sheets read them there. */
  private applyLook() {
    const root = document.documentElement;
    root.dataset.motion = this.motion;
    root.dataset.links = this.linkUnderline;
    // A 12-hour time needs a wider column in the compact view.
    root.style.setProperty('--time-col', this.timeFormat === '24h' ? '48px' : '68px');
    root.style.setProperty('--group-gap', GROUP_GAP[this.groupSpacing]);
  }

  private applyFont() {
    document.documentElement.style.setProperty('--message-size', `${this.fontSize}px`);
    document.documentElement.style.setProperty('--message-line', `${Math.round(this.fontSize * 1.45)}px`);
  }
}

export const prefs = new Prefs();
