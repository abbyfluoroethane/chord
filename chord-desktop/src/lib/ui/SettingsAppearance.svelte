<script lang="ts">
  import Check from 'lucide-svelte/icons/check';
  import Avatar from './Avatar.svelte';
  import Toggle from './Toggle.svelte';
  import Emoji from './Emoji.svelte';
  import Icon from './Icon.svelte';
  import Segmented from './Segmented.svelte';
  import ImportThemeModal from './ImportThemeModal.svelte';
  import ThemeList from './ThemeList.svelte';
  import { EMOJI_PACKS, emojiPacks } from './emojipacks.svelte';
  import SettingRow from './SettingRow.svelte';
  import { clock } from './format';
  import { prefs } from './prefs.svelte';
  import { ZOOM_MAX, ZOOM_MIN, type AnimateGifs, type GroupSpacing, type LinkUnderline, type MotionMode, type TimeFormat } from './prefsdata';
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
  const spacings: { value: GroupSpacing; label: string }[] = [
    { value: 'small', label: 'Small' },
    { value: 'normal', label: 'Normal' },
    { value: 'large', label: 'Large' }
  ];
  const timeFormats: { value: TimeFormat; label: string }[] = [
    { value: 'system', label: 'System' },
    { value: '12h', label: '12-hour' },
    { value: '24h', label: '24-hour' }
  ];
  const underlines: { value: LinkUnderline; label: string }[] = [
    { value: 'always', label: 'Always' },
    { value: 'hover', label: 'On hover' }
  ];
  const animations: { value: AnimateGifs; label: string }[] = [
    { value: 'always', label: 'Always' },
    { value: 'hover', label: 'On hover' },
    { value: 'never', label: 'Never' }
  ];
  const motions: { value: MotionMode; label: string }[] = [
    { value: 'system', label: 'System' },
    { value: 'reduce', label: 'On' },
    { value: 'full', label: 'Off' }
  ];
  // The sample time is mid afternoon, so the 12-hour and 24-hour formats look different.
  const SAMPLE_TIME = new Date(2026, 0, 15, 15, 4).getTime();
  // The slider shows the drag. The zoom applies when the user lets go.
  let zoomShown = $state(prefs.zoom);
</script>

<SettingRow title="Mode" hint="System follows your device.">
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

<h2 class="section spaced">Messages</h2>
<div class="preview" class:compact={prefs.display === 'compact'} aria-label="Preview of messages">
  <div class="msg first">
    <span class="gutter">
      {#if prefs.display === 'compact'}
        <span class="time meta">{clock(SAMPLE_TIME)}</span>
      {:else}
        <Avatar name="Priya Raman" size={40} cut="var(--surface-200)" />
      {/if}
    </span>
    <div>
      <div class="head">
        <b>Priya Raman</b>
        {#if prefs.display !== 'compact'}<span class="time meta">today {clock(SAMPLE_TIME)}</span>{/if}
      </div>
      <span class="text">Build 0.14.2 is up on the playtest branch.</span>
    </div>
  </div>
  <div class="msg">
    <span class="gutter"></span>
    <span class="text">Notes are on <a href="#preview" onclick={(e) => e.preventDefault()}>lanternworks.example/notes</a></span>
  </div>
  <div class="msg gap">
    <span class="gutter">
      {#if prefs.display === 'compact'}
        <span class="time meta">{clock(SAMPLE_TIME + 60_000)}</span>
      {:else}
        <Avatar name="Kenji Ito" size={40} cut="var(--surface-200)" />
      {/if}
    </span>
    <div>
      <div class="head">
        <b>Kenji Ito</b>
        {#if prefs.display !== 'compact'}<span class="time meta">today {clock(SAMPLE_TIME + 60_000)}</span>{/if}
      </div>
      <span class="text" class:jumbo={prefs.jumboEmoji}><Emoji emoji="👍" /><Emoji emoji="🎉" /></span>
    </div>
  </div>
</div>

<SettingRow title="Message display" hint="Compact hides avatars.">
  <Segmented
    label="Message display"
    value={prefs.display}
    options={modes}
    onchange={(v) => prefs.set('display', v)}
  />
</SettingRow>
<SettingRow title="Group spacing" hint="Gap between messages from different people.">
  <Segmented
    label="Group spacing"
    value={prefs.groupSpacing}
    options={spacings}
    onchange={(v) => prefs.set('groupSpacing', v)}
  />
</SettingRow>
<SettingRow title="Font size" hint="Size of message text.">
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
<SettingRow title="Time format" hint="System follows your device.">
  <Segmented
    label="Time format"
    value={prefs.timeFormat}
    options={timeFormats}
    onchange={(v) => prefs.set('timeFormat', v)}
  />
</SettingRow>
<SettingRow title="Large emoji" hint="Messages with only emoji show large.">
  <Toggle
    checked={prefs.jumboEmoji}
    label="Large emoji"
    onchange={(v) => prefs.set('jumboEmoji', v)}
  />
</SettingRow>
<SettingRow title="Underline links" hint="Applies to links in messages.">
  <Segmented
    label="Underline links"
    value={prefs.linkUnderline}
    options={underlines}
    onchange={(v) => prefs.set('linkUnderline', v)}
  />
</SettingRow>
<SettingRow title="Animate GIFs" hint="Applies to images in messages and link previews.">
  <Segmented
    label="Animate GIFs"
    value={prefs.animateGifs}
    options={animations}
    onchange={(v) => prefs.set('animateGifs', v)}
  />
</SettingRow>

<h2 class="section spaced">Interface</h2>
<SettingRow title="Zoom" hint="Scales the whole app.">
  <div class="size">
    <input
      type="range"
      min={ZOOM_MIN}
      max={ZOOM_MAX}
      step="5"
      aria-label="Zoom"
      value={zoomShown}
      oninput={(e) => (zoomShown = Number(e.currentTarget.value))}
      onchange={(e) => prefs.set('zoom', Number(e.currentTarget.value))}
    />
    <output class="mono">{zoomShown} %</output>
  </div>
</SettingRow>
<SettingRow title="Reduce motion" hint="System follows your device.">
  <Segmented
    label="Reduce motion"
    value={prefs.motion}
    options={motions}
    onchange={(v) => prefs.set('motion', v)}
  />
</SettingRow>
<SettingRow title="Show status dots" hint="On avatars in the channel list, member list and contacts.">
  <Toggle
    checked={prefs.showPresence}
    label="Show status dots"
    onchange={(v) => prefs.set('showPresence', v)}
  />
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
    padding: var(--space-3) 0;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    margin-bottom: var(--space-2);
  }
  .msg {
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr);
    column-gap: var(--space-4);
    padding: 2px var(--space-4);
  }
  .msg.gap {
    margin-top: var(--group-gap, 20px);
  }
  .msg.first {
    margin-top: 0;
  }
  .gutter {
    display: flex;
    justify-content: center;
    padding-top: 2px;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-height: 22px;
  }
  .text {
    font-size: var(--message-size, 15px);
    line-height: var(--message-line, 22px);
  }
  .text.jumbo {
    font-size: 40px;
    line-height: 48px;
  }
  .preview.compact .msg {
    grid-template-columns: var(--time-col, 48px) minmax(0, 1fr);
    column-gap: var(--space-2);
  }
  .preview.compact .msg.gap {
    margin-top: 0;
  }
  .preview.compact .gutter {
    justify-content: flex-end;
    padding-top: 0;
  }
  .preview.compact .time {
    white-space: nowrap;
    line-height: var(--message-line, 22px);
  }
  :global(:root[data-links='hover']) .preview a {
    text-decoration: none;
  }
  :global(:root[data-links='hover']) .preview a:hover {
    text-decoration: underline;
  }
</style>
