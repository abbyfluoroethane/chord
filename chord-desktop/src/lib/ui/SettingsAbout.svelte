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
  import { updates } from './updates.svelte';
  import { CHANNELS, checkedText, megabytes, percentOf, slowerNote, versionLine } from './updatesdata';

  // The version comes from the build (src-tauri/build.rs). The preview shows a sample.
  onMount(() => {
    void updates.load();
  });
  const SOURCE = 'https://github.com/abbyfluoroethane/chord';

  const app = $derived(updates.app);
  const s = $derived(updates.status);
  const channel = $derived(updates.channel);
  const hint = $derived(CHANNELS.find((c) => c.value === channel)?.hint ?? '');
  const note = $derived(app ? slowerNote(channel, app.channel) : null);
  const checked = $derived(checkedText(updates.lastChecked, tick()));
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
  <p class="meta">This is a dev build. It does not update itself.</p>
{:else}
  <SettingRow title="Channel" {hint}>
    <Segmented
      label="Update channel"
      value={channel}
      options={CHANNELS.map((c) => ({ value: c.value, label: c.label }))}
      onchange={(v) => updates.setChannel(v)}
    />
  </SettingRow>
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
      <p class="line">Version {s.info.version} is available.</p>
      {#if s.info.notes}<p class="notes">{s.info.notes}</p>{/if}
      {#if s.info.canInstall}
        <button class="btn btn-primary" onclick={() => void updates.install()}>Install</button>
      {:else if s.info.releaseUrl}
        <p class="meta">This install cannot update itself. Download the new version and install it.</p>
        <a class="btn btn-primary" href={s.info.releaseUrl} target="_blank" rel="noopener noreferrer">Download</a>
      {/if}
    {:else if s.kind === 'downloading'}
      {@const pct = percentOf(s.downloaded, s.total)}
      <p class="line">
        Downloading version {s.info.version}…
        {pct === null ? megabytes(s.downloaded) : `${pct}%`}
      </p>
      <progress max="100" value={pct ?? undefined} aria-label="Download"></progress>
    {:else if s.kind === 'ready'}
      <p class="line">Version {s.info.version} is installed.</p>
      <button class="btn btn-primary" onclick={() => void updates.restart()}>Restart to update</button>
    {:else if s.kind === 'failed'}
      <p class="line">{s.info ? 'The update failed.' : 'Cannot check for updates.'}</p>
      <p class="meta">{s.message}</p>
      <button class="btn" onclick={() => updates.retry()}>Retry</button>
    {:else}
      <p class="line">
        {s.kind === 'current' ? 'Chord is up to date.' : prefs.autoUpdate ? 'Chord looks for updates by itself.' : 'Automatic checks are off.'}
      </p>
      <p class="meta">{checked}</p>
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
  progress {
    width: 100%;
    max-width: 320px;
    height: 6px;
    accent-color: var(--brand);
  }
</style>
