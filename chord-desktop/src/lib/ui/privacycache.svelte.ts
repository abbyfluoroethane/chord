// The local caches that the Privacy page shows and clears: the link preview answers and
// the emoji that Chord drew to PNG. Live, Rust holds both. In the browser preview the
// numbers are samples, so the page can show its states.
import type { PrivacyCacheInfo } from '$lib/chord/api';
import { api, live } from './bridge';
import { linkPreviews } from './linkpreviews.svelte';

const SAMPLE: PrivacyCacheInfo = { previewEntries: 14, emojiFiles: 120, emojiBytes: 1_800_000 };

class PrivacyCache {
  info = $state<PrivacyCacheInfo | null>(null);

  async refresh() {
    if (!live) {
      this.info ??= { ...SAMPLE };
      return;
    }
    try {
      this.info = await (await api()).privacyCacheInfo();
    } catch {
      this.info = null;
    }
  }

  async clear(kind: 'previews' | 'emoji') {
    if (kind === 'previews') linkPreviews.clearCache();
    if (live) {
      try {
        await (await api()).clearPrivacyCache(kind);
      } catch {
        /* The next refresh shows what is left. */
      }
      await this.refresh();
      return;
    }
    if (!this.info) return;
    this.info =
      kind === 'previews'
        ? { ...this.info, previewEntries: 0 }
        : { ...this.info, emojiFiles: 0, emojiBytes: 0 };
  }
}

export const privacyCache = new PrivacyCache();
