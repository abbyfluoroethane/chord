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
import { LEGACY_KEY, SETTINGS_KEY, resolvePrefs, type Saved } from './prefsdata';

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
  }

  /** Change one setting, save all, and apply the font size. */
  set<K extends keyof Saved>(key: K, value: Saved[K]) {
    (this as unknown as Saved)[key] = value;
    this.save();
    if (key === 'fontSize') this.applyFont();
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
      emojiPack: this.emojiPack
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

  private applyFont() {
    document.documentElement.style.setProperty('--message-size', `${this.fontSize}px`);
    document.documentElement.style.setProperty('--message-line', `${Math.round(this.fontSize * 1.45)}px`);
  }
}

export const prefs = new Prefs();
