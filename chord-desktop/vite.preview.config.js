// Builds the static UI preview (see preview/main.ts). Relative paths, one page.
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  root: 'preview',
  base: './',
  plugins: [svelte()],
  resolve: {
    alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) },
  },
  build: { outDir: '../build-preview', emptyOutDir: true },
});
