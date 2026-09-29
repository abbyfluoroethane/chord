<script lang="ts">
  import Segmented from './Segmented.svelte';
  import SettingRow from './SettingRow.svelte';
  import { prefs } from './prefs.svelte';
  import { theme, type ThemeChoice } from '$lib/theme/theme.svelte';
  import type { DisplayMode } from './types';

  const themes: { value: ThemeChoice; label: string }[] = [
    { value: 'dark', label: 'Dark' },
    { value: 'light', label: 'Light' },
    { value: 'system', label: 'System' }
  ];
  const modes: { value: DisplayMode; label: string }[] = [
    { value: 'cozy', label: 'Cozy' },
    { value: 'compact', label: 'Compact' }
  ];
</script>

<SettingRow title="Theme" hint="System follows your device.">
  <Segmented label="Theme" value={theme.choice} options={themes} onchange={(v) => theme.set(v)} />
</SettingRow>
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
