<script lang="ts">
  // The version, the update channel and the update state, the source link, and the licences.
  // The updater state lives in updates.svelte.ts, so the banner and this page agree.
  import { onMount } from 'svelte';
  import ChordMark from './ChordMark.svelte';
  import Segmented from './Segmented.svelte';
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { tick } from './now.svelte';
  import { prefs } from './prefs.svelte';
  import { copyText } from './clipboard';
  import { updates } from './updates.svelte';
  import type { UpdateChannel } from '$lib/chord/types';
  import {
    CHANNELS,
    branchChannel,
    branchLabel,
    channelLabel,
    checkedText,
    megabytes,
    offText,
    percentOf,
    slowerNote,
    switchFor,
    updateLine,
    versionLine
  } from './updatesdata';

  // The version comes from the build (src-tauri/build.rs). The preview shows a sample.
  onMount(() => {
    void updates.load();
  });
  const SOURCE = 'https://github.com/abbyfluoroethane/chord';

  const app = $derived(updates.app);
  const s = $derived(updates.status);
  const channel = $derived(updates.channel);
  const hint = $derived(CHANNELS.find((c) => c.value === channel)?.hint ?? '');
  const note = $derived(app && !app.flatpak ? slowerNote(channel, app.channel) : null);
  const checked = $derived(checkedText(updates.lastChecked, tick()));

  // In a Flatpak the branch is the channel. A pick shows the command that installs the other
  // branch; it does not switch.
  const flatpak = $derived(app?.flatpak ?? null);
  const installed = $derived(flatpak ? branchChannel(flatpak.branch) : null);
  let wanted = $state<UpdateChannel | null>(null);
  const shown = $derived(wanted ?? installed ?? ('' as UpdateChannel));
  const howTo = $derived(wanted && wanted !== installed ? switchFor(app, wanted) : null);
  const flatpakHint = $derived(
    flatpak ? `The Flatpak branch that you installed: ${branchLabel(flatpak.branch)}.` : ''
  );
</script>

<div class="head">
  <span class="mark"><ChordMark /></span>
  <div>
    <p class="name">Chord Desktop</p>
    <p class="meta">{app ? versionLine(app) : 'Version unknown'}</p>
  </div>
</div>

<p>
  Chord is open source. Read the code, report a problem, or send a fix at
  <a href={SOURCE} target="_blank" rel="noopener noreferrer">{SOURCE}</a>.
</p>

<h2 class="section">Updates</h2>
{#if s.kind === 'off'}
  <p class="meta">{app ? offText(app) : 'This build does not update itself.'}</p>
{:else}
  {#if flatpak}
    <SettingRow title="Channel" hint={flatpakHint}>
      <Segmented
        label="Update channel"
        value={shown}
        options={CHANNELS.map((c) => ({ value: c.value, label: c.label }))}
        onchange={(v) => (wanted = v === installed ? null : v)}
      />
    </SettingRow>
    {#if howTo}
      <div class="switch">
        <p>
          To get {channelLabel(howTo.channel)}, install its Flatpak branch once. Run this command in a terminal:
        </p>
        <pre><code>{howTo.install}</code></pre>
        <div class="row">
          <button class="btn" onclick={() => void copyText(howTo.install, 'Command copied.')}>Copy</button>
          <button class="btn" onclick={() => (wanted = null)}>Cancel</button>
        </div>
        <p class="meta">
          Both branches can stay installed side by side, and they use the same data. To pick the one that
          runs, use <code>{howTo.makeCurrent}</code>.
        </p>
      </div>
    {/if}
  {:else}
    <SettingRow title="Channel" {hint}>
      <Segmented
        label="Update channel"
        value={channel}
        options={CHANNELS.map((c) => ({ value: c.value, label: c.label }))}
        onchange={(v) => updates.setChannel(v)}
      />
    </SettingRow>
  {/if}
  {#if note}<p class="meta note">{note}</p>{/if}
  <SettingRow title="Check automatically" hint="When Chord starts and once a day.">
    <Toggle
      checked={prefs.autoUpdate}
      label="Check automatically"
      onchange={(v) => {
        prefs.set('autoUpdate', v);
        if (v) void updates.maybeCheck();
      }}
    />
  </SettingRow>

  <div class="status" aria-live="polite">
    {#if s.kind === 'checking'}
      <p class="line">Checking for updates…</p>
    {:else if s.kind === 'available'}
      <p class="line">{updateLine(s.info, 'available')}</p>
      {#if s.info.notes}<p class="notes">{s.info.notes}</p>{/if}
      <button class="btn btn-primary" onclick={() => void updates.install()}>Install</button>
    {:else if s.kind === 'downloading'}
      {@const pct = percentOf(s.downloaded, s.total)}
      <p class="line">
        {updateLine(s.info, 'downloading')}
        {pct === null ? megabytes(s.downloaded) : `${pct}%`}
      </p>
      <progress max="100" value={pct ?? undefined} aria-label="Download"></progress>
    {:else if s.kind === 'ready'}
      <p class="line">{updateLine(s.info, 'ready')}</p>
      <button class="btn btn-primary" onclick={() => void updates.restart()}>Restart to update</button>
    {:else if s.kind === 'failed'}
      <p class="line">{s.info ? 'The update failed.' : 'Cannot check for updates.'}</p>
      <p class="meta">{s.message}</p>
      <button class="btn" onclick={() => updates.retry()}>Retry</button>
    {:else}
      <p class="line">
        {s.kind === 'current' ? 'Chord is up to date.' : prefs.autoUpdate ? 'Chord looks for updates by itself.' : 'Automatic checks are off.'}
      </p>
      <p class="meta">
        {checked}{flatpak ? '. Flatpak also looks for updates by itself, twice an hour.' : ''}
      </p>
      <button class="btn" onclick={() => void updates.check()}>Check for updates</button>
    {/if}
  </div>
{/if}

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
  .note {
    margin: var(--space-2) 0 0;
  }
  .status {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3) 0;
  }
  .status p {
    margin: 0;
  }
  .line {
    font-weight: 500;
  }
  .notes {
    white-space: pre-wrap;
    color: var(--ink-muted);
  }
  .switch {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    margin: var(--space-2) 0 0;
    padding: var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .switch p {
    margin: 0;
  }
  .switch pre {
    align-self: stretch;
    margin: 0;
    padding: var(--space-2);
    background: var(--surface-100);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  code {
    font-family: var(--font-mono);
    font-size: 12px;
    user-select: text;
  }
  .row {
    display: flex;
    gap: var(--space-2);
  }
  progress {
    width: 100%;
    max-width: 320px;
    height: 6px;
    accent-color: var(--brand);
  }
</style>
