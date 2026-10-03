<script lang="ts">
  // Chat. How you write, what shows in a message, and when Chord asks first. Each switch
  // lives in the prefs store and works at once: the composer, the message rows and the
  // attachment view read it from there.
  import Segmented from './Segmented.svelte';
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { prefs } from './prefs.svelte';
  import type { SendKey } from './prefsdata';

  const sendKeys: { value: SendKey; label: string }[] = [
    { value: 'enter', label: 'Enter' },
    { value: 'mod-enter', label: 'Ctrl+Enter' }
  ];
</script>

<h2 class="section">Writing</h2>
<SettingRow title="Send with" hint="Ctrl+Enter is Cmd+Enter on a Mac.">
  <Segmented
    label="Send with"
    value={prefs.sendKey}
    options={sendKeys}
    onchange={(v) => prefs.set('sendKey', v)}
  />
</SettingRow>
<SettingRow title="Spell check" hint="Underline misspelled words in the message box.">
  <Toggle checked={prefs.spellcheck} label="Spell check" onchange={(v) => prefs.set('spellcheck', v)} />
</SettingRow>
<SettingRow title="Convert emoticons" hint="Send :) and <3 as emoji.">
  <Toggle checked={prefs.emoticons} label="Convert emoticons" onchange={(v) => prefs.set('emoticons', v)} />
</SettingRow>

<h2 class="section">Media</h2>
<SettingRow title="Show photos and videos" hint="Attachments show as plain links when disabled.">
  <Toggle
    checked={prefs.inlineMedia}
    label="Show photos and videos"
    onchange={(v) => prefs.set('inlineMedia', v)}
  />
</SettingRow>
<SettingRow title="Autoplay videos" hint="Videos start without sound when they load.">
  <Toggle
    checked={prefs.autoplayVideo}
    label="Autoplay videos"
    onchange={(v) => prefs.set('autoplayVideo', v)}
  />
</SettingRow>
<SettingRow title="Show spoilers" hint="Show hidden text without a click.">
  <Toggle
    checked={prefs.showSpoilers}
    label="Show spoilers"
    onchange={(v) => prefs.set('showSpoilers', v)}
  />
</SettingRow>

<h2 class="section">Messages</h2>
<SettingRow title="Confirm before deleting" hint="Hold Shift when you delete to skip the question.">
  <Toggle
    checked={prefs.confirmDelete}
    label="Confirm before deleting"
    onchange={(v) => prefs.set('confirmDelete', v)}
  />
</SettingRow>

<style>
  .section {
    margin: var(--space-6) 0 var(--space-2);
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .section:first-child {
    margin-top: 0;
  }
</style>
