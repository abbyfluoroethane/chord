// Link previews: the switch in the privacy settings, and one answer for each URL.
// Live, the answer comes from the `link_preview` command (Rust caches it too). In the
// browser preview it comes from the sample data, with no network.
import type { LinkPreview } from '$lib/chord/types';
import * as fx from '$lib/fixtures/data';
import { api, live } from './bridge';
import { settings } from './local';

const KEY = 'linkPreviews';

/** `undefined` while the answer is on its way, `null` when there is no preview. */
type Entry = LinkPreview | null | undefined;

class LinkPreviews {
  /** The "Show link previews" switch. On by default. */
  enabled = $state(true);
  private entries = $state<Record<string, Entry>>({});
  private started = new Set<string>();

  load() {
    if (live) {
      this.enabled = settings.get<boolean>(KEY) ?? true;
      return;
    }
    try {
      const raw = localStorage.getItem('chord.' + KEY);
      if (raw !== null) this.enabled = raw !== 'false';
    } catch {
      /* storage blocked, keep the default */
    }
  }

  setEnabled(on: boolean) {
    this.enabled = on;
    if (live) {
      settings.set(KEY, on);
      return;
    }
    try {
      localStorage.setItem('chord.' + KEY, String(on));
    } catch {
      /* ignore */
    }
  }

  /** The answer for `url` so far. Call `request` to get it. */
  get(url: string): Entry {
    return this.entries[url];
  }

  /** Ask for `url` once. A failed request counts as no preview until the app restarts. */
  request(url: string) {
    if (!this.enabled || this.started.has(url)) return;
    this.started.add(url);
    void this.fetch(url).then((p) => (this.entries[url] = p));
  }

  private async fetch(url: string): Promise<LinkPreview | null> {
    if (!live) {
      await new Promise((r) => setTimeout(r, 150));
      return fx.linkPreviews[url] ?? null;
    }
    try {
      return await (await api()).linkPreview(url);
    } catch {
      return null;
    }
  }
}

export const linkPreviews = new LinkPreviews();
