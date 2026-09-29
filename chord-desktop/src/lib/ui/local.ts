// Local settings that live in the app config directory (get_settings and set_settings).
// The UI owns the shape. Unknown keys stay as they are, so a newer version keeps its data.
// Outside Tauri the calls do nothing and the callers use localStorage instead.
import type { Settings } from '$lib/chord/types';
import { api, live } from './bridge';

class LocalSettings {
  private cache: Settings = {};
  private timer: ReturnType<typeof setTimeout> | undefined;
  private saving: Promise<void> = Promise.resolve();

  /** Read the file once, at start. A read that fails leaves an empty object. */
  async load(): Promise<Settings> {
    if (!live) return this.cache;
    try {
      this.cache = (await (await api()).getSettings()) ?? {};
    } catch {
      this.cache = {};
    }
    return this.cache;
  }

  get<T>(key: string): T | undefined {
    return this.cache[key] as T | undefined;
  }

  /** Change one key. The write happens a moment later and keeps the other keys. */
  set(key: string, value: unknown) {
    this.cache = { ...this.cache, [key]: value };
    if (!live) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.flush(), 200);
  }

  /** Write now. */
  flush(): Promise<void> {
    clearTimeout(this.timer);
    this.saving = this.saving.then(async () => {
      try {
        await (await api()).setSettings(this.cache);
      } catch {
        /* The next change tries again. */
      }
    });
    return this.saving;
  }
}

export const settings = new LocalSettings();
