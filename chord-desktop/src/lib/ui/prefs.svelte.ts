// User settings that live on this device. One JSON blob in localStorage.
// The bridge can move them to get_settings and set_settings later.
import type { DisplayMode, NotificationLevel } from './types';

const KEY = 'chord.prefs';

interface Saved {
  desktopNotifications: boolean;
  sound: boolean;
  dmLevel: NotificationLevel;
  channelLevel: NotificationLevel;
  muteDms: boolean;
  autoApprove: boolean;
  display: DisplayMode;
  fontSize: number;
}

const LEVELS: NotificationLevel[] = ['all', 'mentions', 'nothing'];

class Prefs {
  desktopNotifications = $state(true);
  sound = $state(true);
  dmLevel = $state<NotificationLevel>('all');
  channelLevel = $state<NotificationLevel>('mentions');
  muteDms = $state(false);
  autoApprove = $state(false);
  display = $state<DisplayMode>('cozy');
  fontSize = $state(15);

  load() {
    try {
      const raw = localStorage.getItem(KEY);
      if (raw) {
        const v = JSON.parse(raw) as Partial<Saved>;
        if (typeof v.desktopNotifications === 'boolean') this.desktopNotifications = v.desktopNotifications;
        if (typeof v.sound === 'boolean') this.sound = v.sound;
        if (v.dmLevel && LEVELS.includes(v.dmLevel)) this.dmLevel = v.dmLevel;
        if (v.channelLevel && LEVELS.includes(v.channelLevel)) this.channelLevel = v.channelLevel;
        if (typeof v.muteDms === 'boolean') this.muteDms = v.muteDms;
        if (typeof v.autoApprove === 'boolean') this.autoApprove = v.autoApprove;
        if (v.display === 'cozy' || v.display === 'compact') this.display = v.display;
        if (typeof v.fontSize === 'number') this.fontSize = Math.min(20, Math.max(12, v.fontSize));
      }
    } catch {
      /* storage blocked or bad JSON, keep the defaults */
    }
    this.applyFont();
  }

  /** Change one setting, save all, and apply the font size. */
  set<K extends keyof Saved>(key: K, value: Saved[K]) {
    (this as unknown as Saved)[key] = value;
    try {
      const out: Saved = {
        desktopNotifications: this.desktopNotifications,
        sound: this.sound,
        dmLevel: this.dmLevel,
        channelLevel: this.channelLevel,
        muteDms: this.muteDms,
        autoApprove: this.autoApprove,
        display: this.display,
        fontSize: this.fontSize
      };
      localStorage.setItem(KEY, JSON.stringify(out));
    } catch {
      /* ignore */
    }
    if (key === 'fontSize') this.applyFont();
  }

  private applyFont() {
    document.documentElement.style.setProperty('--message-size', `${this.fontSize}px`);
    document.documentElement.style.setProperty('--message-line', `${Math.round(this.fontSize * 1.45)}px`);
  }
}

export const prefs = new Prefs();
