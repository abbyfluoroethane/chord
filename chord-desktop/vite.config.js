import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error type error without @types/node package
import { readFileSync } from "node:fs";
const host = process.env.TAURI_DEV_HOST;

/**
 * Slim the Emojibase list. The picker needs only a few fields of each entry, and it skips
 * group 2 (the skin tone swatches). The Raw type in src/lib/ui/emojidata.ts lists the fields.
 * The module stays in memory after the first load, so a smaller list saves both bytes and heap.
 */
/** @typedef {{ emoji: string, label: string, tags?: string[], group?: number, order?: number, version: number, skins?: { emoji: string, tone: number | number[] }[] }} Emoji */

/** @returns {import("vite").Plugin} */
function slimEmojibase() {
  return {
    name: "chord-slim-emojibase",
    enforce: "pre",
    async resolveId(source, importer, options) {
      if (source !== "emojibase-data/en/data.json") return null;
      const found = await this.resolve(source, importer, { ...options, skipSelf: true });
      return found ? `\0chord-emojibase:${found.id}.js` : null;
    },
    load(id) {
      if (!id.startsWith("\0chord-emojibase:")) return null;
      const file = id.slice("\0chord-emojibase:".length, -3);
      this.addWatchFile(file);
      /** @type {Emoji[]} */
      const all = JSON.parse(readFileSync(file, "utf8"));
      const slim = all
        .filter((e) => e.group !== undefined && e.group !== 2)
        .map((e) => ({
          emoji: e.emoji,
          label: e.label,
          tags: e.tags,
          group: e.group,
          order: e.order,
          version: e.version,
          skins: e.skins?.map((k) => ({ emoji: k.emoji, tone: k.tone })),
        }));
      return { code: `export default ${JSON.stringify(slim)};`, map: null };
    },
  };
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [slimEmojibase(), sveltekit()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "127.0.0.1",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
