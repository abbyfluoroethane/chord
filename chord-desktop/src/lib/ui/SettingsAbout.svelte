<script lang="ts">
  import { onMount } from 'svelte';
  import ChordMark from './ChordMark.svelte';
  import { api, live } from './bridge';
  import type { AppInfo } from '$lib/chord/types';

  // The version comes from the build (src-tauri/build.rs). The preview shows a sample.
  let app = $state<AppInfo | null>(null);
  onMount(() => {
    void (async () => {
      try {
        app = live ? await (await api()).appInfo() : { version: '0.1.0', commit: 'preview', os: 'preview', arch: 'browser' };
      } catch {
        app = null;
      }
    })();
  });
  const SOURCE = 'https://github.com/abbyfluoroethane/chord';
</script>

<div class="head">
  <span class="mark"><ChordMark /></span>
  <div>
    <p class="name">Chord Desktop</p>
    <p class="meta">{app ? `Version ${app.version} (${app.commit})` : 'Version unknown'}</p>
  </div>
</div>

<p>
  Chord is open source. Read the code, report a problem, or send a fix at
  <a href={SOURCE} target="_blank" rel="noopener noreferrer">{SOURCE}</a>.
</p>

<h2 class="section">Licences</h2>
<ul>
  <li>IBM Plex Sans and IBM Plex Mono, SIL Open Font License.</li>
  <li>Bricolage Grotesque, SIL Open Font License.</li>
  <li>Lucide icons, ISC License.</li>
  <li>PhotoSwipe image viewer, MIT License.</li>
  <li>Video.js media player, Apache License 2.0.</li>
  <li>Emojibase emoji data, MIT License.</li>
  <li>Twemoji graphics by Twitter and contributors, CC BY 4.0.</li>
  <li>Noto Emoji by Google, Apache License 2.0, from the Iconify set.</li>
  <li>Fluent Emoji by Microsoft, MIT License, from the Iconify set.</li>
  <li>Catppuccin Mocha and Latte colours by Catppuccin, MIT License.</li>
  <li>highlight.js code highlighting, BSD 3-Clause License.</li>
  <li>GIF search powered by KLIPY.</li>
</ul>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }
  .mark {
    display: block;
    width: 48px;
    height: 48px;
  }
  p {
    margin: 0 0 var(--space-3);
  }
  .head p {
    margin: 0;
  }
  .name {
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 20px;
    line-height: 26px;
  }
  .section {
    margin: var(--space-6) 0 var(--space-2);
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  ul {
    margin: 0;
    padding-left: var(--space-6);
    color: var(--ink-muted);
  }
</style>
