// Emoji packs: which images Chord draws for emoji. In the app, Rust serves each emoji as
// an SVG file over chord-emoji:// (src-tauri/src/emoji.rs). The browser preview has no
// Rust: it registers a Twemoji source of its own (preview/emoji-source.ts).
import { convertFileSrc } from '@tauri-apps/api/core';
import { plainError } from './adapt';
import { api, live } from './bridge';
import type { EmojiPackId } from './emojipackids';
import { prefs } from './prefs.svelte';
import { ui } from './ui.svelte';

export const EMOJI_SCHEME = 'chord-emoji';

export interface EmojiPackInfo {
  id: EmojiPackId;
  name: string;
  /** Who made it, and its license. Shown in the settings. */
  credit: string;
  /** The download size, or null if the pack needs no download. */
  download: string | null;
}

export const EMOJI_PACKS: EmojiPackInfo[] = [
  { id: 'twemoji', name: 'Twemoji', credit: 'Twitter and contributors, CC BY 4.0', download: null },
  { id: 'noto', name: 'Noto Emoji', credit: 'Google, Apache License 2.0', download: '3 MB' },
  { id: 'fluent', name: 'Fluent Emoji', credit: 'Microsoft, MIT License', download: '14 MB' },
  { id: 'system', name: 'System', credit: 'The emoji font of this computer', download: null }
];

/**
 * The file name of an emoji in a pack: the code points in lower-case hex, at least 4
 * digits, joined with "-", without U+FE0F (the emoji presentation selector).
 */
export function emojiHex(emoji: string): string {
  return [...emoji]
    .map((c) => c.codePointAt(0) ?? 0)
    .filter((cp) => cp !== 0xfe0f)
    .map((cp) => cp.toString(16).padStart(4, '0'))
    .join('-');
}

/** A source of emoji images for the browser preview: a data URL for a hex name. */
type PreviewSource = (hex: string) => string | null;
let previewSource: PreviewSource | null = null;

class EmojiPacks {
  /** Which packs are on this computer. Twemoji and System are always there. */
  installed = $state<Record<string, boolean>>({ twemoji: true, system: true });
  /** The pack that downloads now, if any. */
  installing = $state<EmojiPackId | null>(null);
  /** Goes up when the preview source loads, so that emoji draw again. */
  version = $state(0);

  /** Read which packs are installed. Maps to api.emojiPacks(). */
  async load() {
    if (!live) return;
    try {
      for (const p of await (await api()).emojiPacks()) {
        this.installed[p.id] = p.installed || p.bundled;
      }
    } catch {
      /* keep the defaults */
    }
  }

  /** Use a pack. A pack that is not on this computer downloads first. */
  async choose(id: EmojiPackId) {
    if (!live && id !== 'twemoji' && id !== 'system') {
      ui.say('Noto and Fluent download in the app. The preview has Twemoji and System only.');
      return;
    }
    if (!this.installed[id]) {
      this.installing = id;
      try {
        await (await api()).emojiPackInstall(id);
        this.installed[id] = true;
      } catch (e) {
        ui.say(plainError(e));
        return;
      } finally {
        this.installing = null;
      }
    }
    prefs.set('emojiPack', id);
  }

  /** The pack that draws now: the chosen one, or Twemoji while it downloads. */
  get active(): EmojiPackId {
    const id = prefs.emojiPack;
    return this.installed[id] ? id : 'twemoji';
  }

  /** The image for an emoji in `pack`, or null to draw it with the system font. */
  url(emoji: string, pack: EmojiPackId = this.active): string | null {
    if (pack === 'system') return null;
    const hex = emojiHex(emoji);
    if (!live) {
      void this.version;
      return previewSource?.(hex) ?? null;
    }
    return convertFileSrc(`${pack}/${hex}.svg`, EMOJI_SCHEME);
  }

  /** The browser preview gives its Twemoji source here. */
  setPreviewSource(source: PreviewSource) {
    previewSource = source;
    this.version += 1;
  }
}

export const emojiPacks = new EmojiPacks();

export { splitEmoji, type EmojiPart } from './emojisplit';
