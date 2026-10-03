<script lang="ts">
  // Notifications. The core applies the level of each chat (set in the chat menus). These
  // settings narrow the notice: Rust reads the notice switches from the saved settings
  // (notify.rs), and the sound plays in the page (notices.ts). There is no default level
  // here: the core has no global default, so a setting for it would do nothing.
  import SettingRow from './SettingRow.svelte';
  import Segmented from './Segmented.svelte';
  import Toggle from './Toggle.svelte';
  import { live, api } from './bridge';
  import { SOUNDS, beep, fromTimeText, toTimeText } from './notices';
  import { prefs } from './prefs.svelte';

  let testState = $state('');

  /** Show a system notice and play the sound, so the user can check both. */
  async function sendTest() {
    testState = '';
    beep(prefs.soundChoice, prefs.soundVolume);
    if (!live) {
      testState = 'The preview has no system notices.';
      return;
    }
    try {
      await (await api()).sendTestNotice();
      testState = 'Sent.';
    } catch {
      testState = 'The system refused the notice.';
    }
  }

  function setTime(key: 'quietFrom' | 'quietTo', text: string) {
    const m = fromTimeText(text);
    if (m !== null) prefs.set(key, m);
  }
</script>

<SettingRow title="Desktop notifications" hint="Notify when Chord is in the background.">
  <Toggle
    checked={prefs.desktopNotifications}
    label="Desktop notifications"
    onchange={(v) => prefs.set('desktopNotifications', v)}
  />
</SettingRow>
<SettingRow title="Show message text" hint="Notices show only the sender when disabled.">
  <Toggle
    checked={prefs.noticePreview}
    label="Show message text"
    onchange={(v) => prefs.set('noticePreview', v)}
  />
</SettingRow>
<SettingRow title="Test notice" hint={testState || 'Send a notice and play the sound now.'}>
  <button class="btn" onclick={sendTest}>Send a test notice</button>
</SettingRow>
<SettingRow title="Mute messages from people" hint="Silence direct messages. Channels still notify.">
  <Toggle checked={prefs.muteDms} label="Mute messages from people" onchange={(v) => prefs.set('muteDms', v)} />
</SettingRow>

<h2 class="section">Quiet hours</h2>
<SettingRow title="Quiet hours" hint="No notices and no sound between two times each day.">
  <Toggle checked={prefs.quietHours} label="Quiet hours" onchange={(v) => prefs.set('quietHours', v)} />
</SettingRow>
{#if prefs.quietHours}
  <SettingRow title="From">
    <input
      type="time"
      aria-label="Quiet hours start"
      value={toTimeText(prefs.quietFrom)}
      onchange={(e) => setTime('quietFrom', e.currentTarget.value)}
    />
  </SettingRow>
  <SettingRow title="Until">
    <input
      type="time"
      aria-label="Quiet hours end"
      value={toTimeText(prefs.quietTo)}
      onchange={(e) => setTime('quietTo', e.currentTarget.value)}
    />
  </SettingRow>
{/if}

<h2 class="section">Sound</h2>
<SettingRow title="Sound" hint="Play a sound for new messages.">
  <Toggle checked={prefs.sound} label="Sound" onchange={(v) => prefs.set('sound', v)} />
</SettingRow>
{#if prefs.sound}
  <SettingRow title="Sound choice" hint="Chord makes each sound itself.">
    <Segmented
      label="Sound choice"
      value={prefs.soundChoice}
      options={SOUNDS}
      onchange={(v) => {
        prefs.set('soundChoice', v);
        beep(v, prefs.soundVolume);
      }}
    />
  </SettingRow>
  <SettingRow title="Volume">
    <div class="size">
      <input
        type="range"
        min="0"
        max="100"
        step="5"
        aria-label="Volume"
        value={prefs.soundVolume}
        oninput={(e) => prefs.set('soundVolume', Number(e.currentTarget.value))}
        onchange={() => beep(prefs.soundChoice, prefs.soundVolume)}
      />
      <output class="mono">{prefs.soundVolume}%</output>
    </div>
  </SettingRow>
{/if}

<h2 class="section">Unread count</h2>
<SettingRow title="Unread badge" hint="Show the count on the app icon and in the window title.">
  <Toggle checked={prefs.unreadBadge} label="Unread badge" onchange={(v) => prefs.set('unreadBadge', v)} />
</SettingRow>

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
  input[type='time'] {
    font: inherit;
    color: var(--ink);
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 4px 8px;
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
</style>
