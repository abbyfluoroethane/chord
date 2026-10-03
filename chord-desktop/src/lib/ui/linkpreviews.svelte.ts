// Link previews: the switch in the privacy settings, and one answer for each URL.
// Live, the answer comes from the `link_preview` command (Rust caches it too). In the
// browser preview it comes from the sample data, with no network.
import type { LinkPreview } from '$lib/chord/types';
import * as fx from '$lib/fixtures/data';
import { api, live } from './bridge';
import { settings } from './local';

const KEY = 'linkPreviews';
const STRANGERS_KEY = 'loadFromStrangers';

/** `undefined` while the answer is on its way, `null` when there is no preview. */
type Entry = LinkPreview | null | undefined;

class LinkPreviews {
  /** The "Show link previews" switch. On by default. */
  enabled = $state(true);
  /**
   * "Load files from people who are not contacts". On by default. When it is off, a message
   * from a stranger shows no embeds, because loading one tells the sender the IP address and
   * the time of reading.
   */
  strangers = $state(true);
  private entries = $state<Record<string, Entry>>({});
  private started = new Set<string>();
  /** The preview images by their address: a `data:` URL, `null` if it failed. */
  private images = $state<Record<string, string | null | undefined>>({});
  private imagesStarted = new Set<string>();

  load() {
    if (live) {
      this.enabled = settings.get<boolean>(KEY) ?? true;
      this.strangers = settings.get<boolean>(STRANGERS_KEY) ?? true;
      return;
    }
    try {
      const raw = localStorage.getItem('chord.' + KEY);
      if (raw !== null) this.enabled = raw !== 'false';
      this.strangers = localStorage.getItem('chord.' + STRANGERS_KEY) !== 'false';
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

  setStrangers(on: boolean) {
    this.strangers = on;
    if (live) {
      settings.set(STRANGERS_KEY, on);
      return;
    }
    try {
      localStorage.setItem('chord.' + STRANGERS_KEY, String(on));
    } catch {
      /* ignore */
    }
  }

  /** Forget all answers and images. The next `request` asks again. */
  clearCache() {
    this.entries = {};
    this.started = new Set();
    this.images = {};
    this.imagesStarted = new Set();
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

  /**
   * What to put in an `<img>` for the preview image at `url`: `undefined` while it loads,
   * `null` if it failed. Live, the bridge fetches it and the answer is a `data:` URL. The
   * webview never loads a remote address for a preview. Call `requestImage` first.
   */
  image(url: string): string | null | undefined {
    return live ? this.images[url] : url;
  }

  requestImage(url: string) {
    if (!live || this.imagesStarted.has(url)) return;
    this.imagesStarted.add(url);
    void (async () => {
      try {
        this.images[url] = await (await api()).linkImage(url);
      } catch {
        this.images[url] = null;
      }
    })();
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
