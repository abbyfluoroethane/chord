// The Twemoji images for the browser preview, which has no Rust to serve them. The app
// never loads this file. The two JSON files load as assets, not as code.
import charsUrl from '@iconify-json/twemoji/chars.json?url';
import iconsUrl from '@iconify-json/twemoji/icons.json?url';
import { emojiPacks } from '$lib/ui/emojipacks.svelte';

interface IconSet {
  width?: number;
  height?: number;
  icons: Record<string, { body: string; width?: number; height?: number }>;
  aliases?: Record<string, { parent: string }>;
}

export async function loadPreviewEmoji(): Promise<void> {
  const [chars, set] = await Promise.all([
    fetch(charsUrl).then((r) => r.json() as Promise<Record<string, string>>),
    fetch(iconsUrl).then((r) => r.json() as Promise<IconSet>)
  ]);
  const cache = new Map<string, string | null>();
  emojiPacks.setPreviewSource((hex) => {
    if (cache.has(hex)) return cache.get(hex) ?? null;
    const name = chars[hex];
    const icon = name ? (set.icons[name] ?? set.icons[set.aliases?.[name]?.parent ?? '']) : undefined;
    let url: string | null = null;
    if (icon) {
      const w = icon.width ?? set.width ?? 36;
      const h = icon.height ?? set.height ?? 36;
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}">${icon.body}</svg>`;
      url = `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
    }
    cache.set(hex, url);
    return url;
  });
}
