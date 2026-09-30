<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import Emoji from './Emoji.svelte';
  import Icon from './Icon.svelte';
  import Segmented from './Segmented.svelte';
  import ImportThemeModal from './ImportThemeModal.svelte';
  import ThemeList from './ThemeList.svelte';
  import { EMOJI_PACKS, emojiPacks } from './emojipacks.svelte';
  import SettingRow from './SettingRow.svelte';
  import { prefs } from './prefs.svelte';
  import { theme, type ThemeChoice } from '$lib/theme/theme.svelte';
  import type { DisplayMode } from './types';

  const themes: { value: ThemeChoice; label: string }[] = [
    { value: 'dark', label: 'Dark' },
    { value: 'light', label: 'Light' },
    { value: 'system', label: 'System' }
  ];
  const SAMPLE = ['😀', '👋🏽', '❤️', '🎉', '🚀'];
  let importing = $state(false);

  const modes: { value: DisplayMode; label: string }[] = [
    { value: 'cozy', label: 'Cozy' },
    { value: 'compact', label: 'Compact' }
  ];
</script>

<SettingRow title="Mode" hint="System follows your device, and switches between your dark and light themes.">
  <Segmented label="Mode" value={theme.choice} options={themes} onchange={(v) => theme.set(v)} />
</SettingRow>

<div class="themes-head">
  <h2 class="section">Dark theme</h2>
  <button class="btn" onclick={() => (importing = true)}>Import theme</button>
</div>
<ThemeList mode="dark" />
<h2 class="section">Light theme</h2>
<ThemeList mode="light" />
<div class="after-themes"></div>
{#if importing}<ImportThemeModal onclose={() => (importing = false)} />{/if}

<SettingRow title="Message display" hint="Compact hides avatars and puts times on the left.">
  <Segmented
    label="Message display"
    value={prefs.display}
    options={modes}
    onchange={(v) => prefs.set('display', v)}
  />
</SettingRow>
<SettingRow title="Font size" hint="Size of message text, from 12 to 20 px.">
  <div class="size">
    <input
      type="range"
      min="12"
      max="20"
      step="1"
      aria-label="Font size"
      value={prefs.fontSize}
      oninput={(e) => prefs.set('fontSize', Number(e.currentTarget.value))}
    />
    <output class="mono">{prefs.fontSize} px</output>
  </div>
</SettingRow>

<h2 class="section spaced">Emoji</h2>
<div class="packs" role="radiogroup" aria-label="Emoji style">
  {#each EMOJI_PACKS as p (p.id)}
    {@const on = prefs.emojiPack === p.id}
    {@const ready = emojiPacks.installed[p.id]}
    <button
      class="pack"
      class:on
      role="radio"
      aria-checked={on}
      disabled={emojiPacks.installing !== null}
      onclick={() => emojiPacks.choose(p.id)}
    >
      <span class="pack-head">
        <span class="pack-name">{p.name}</span>
        {#if on}<span class="check"><Icon icon={Check} size={16} /></span>{/if}
      </span>
      <span class="sample" aria-hidden="true">
        {#if emojiPacks.installing === p.id}
          <span class="ring"></span><span class="meta">Downloading</span>
        {:else if ready}
          {#each SAMPLE as e (e)}<Emoji emoji={e} pack={p.id} />{/each}
        {:else}
          <span class="meta">Downloads {p.download} when you pick it</span>
        {/if}
      </span>
      <span class="meta credit">{p.credit}</span>
    </button>
  {/each}
</div>

<h2 class="section">Preview</h2>
<div class="preview" class:compact={prefs.display === 'compact'}>
  <span class="time meta">12:40</span>
  <div>
    <b>Rin</b>
    <span class="text">Static fire moved to 14:00 tomorrow. Bring the checklist.</span>
  </div>
</div>

<style>
  .size {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  input[type='range'] {
    width: 200px;
    accent-color: var(--brand);
  }
  output {
    width: 48px;
    text-align: right;
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
  .after-themes {
    height: var(--space-6);
  }
  .themes-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    margin-top: var(--space-6);
  }
  .themes-head .section {
    margin: 0 0 var(--space-2);
  }
  .themes-head .btn {
    margin-bottom: var(--space-2);
  }
  .packs {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .pack {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    text-align: left;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    transition:
      border-color var(--dur-fast) var(--ease-out),
      background var(--dur-fast) var(--ease-out);
  }
  .pack:hover:not(:disabled) {
    border-color: var(--ink-muted);
  }
  .pack.on {
    background: var(--selected);
    border-color: var(--ink-muted);
  }
  .pack:disabled {
    cursor: progress;
  }
  .pack-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .pack-name {
    font-weight: 600;
  }
  .check {
    display: grid;
    color: var(--brand);
  }
  .sample {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-height: 32px;
    font-size: 20px;
  }
  .meta {
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.02em;
  }
  .ring {
    width: 16px;
    height: 16px;
    box-sizing: border-box;
    border: 2px solid var(--line);
    border-top-color: var(--brand);
    border-radius: 50%;
    animation: spin 840ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation: none;
    }
  }
  .preview {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .preview:not(.compact) .time {
    display: none;
  }
  .text {
    font-size: var(--message-size, 15px);
    line-height: var(--message-line, 22px);
  }
</style>
