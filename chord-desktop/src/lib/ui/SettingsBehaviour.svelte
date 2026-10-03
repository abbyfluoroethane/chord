<script lang="ts">
  // App behaviour: the login entry, the close button, the tray icon, the window place, and
  // auto-away. Rust reads the first four prefs from the settings file (behaviour.rs). The
  // login entry is not a pref: the OS keeps it, so the page asks the OS. In the browser
  // preview there is no OS, so the switch only changes the page.
  import { onMount } from 'svelte';
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { api, live } from './bridge';
  import { prefs } from './prefs.svelte';
  import { AUTO_AWAY_MINUTES } from './prefsdata';
  import { ui } from './ui.svelte';

  const mac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.userAgent);

  let autostart = $state(false);
  let autostartBusy = $state(false);

  onMount(() => {
    if (!live) return;
    void api()
      .then((b) => b.getAutostart())
      .then((on) => (autostart = on))
      .catch(() => {
        /* Keep it off. */
      });
  });

  async function setAutostart(on: boolean) {
    if (!live) {
      autostart = on;
      return;
    }
    autostartBusy = true;
    try {
      await (await api()).setAutostart(on);
      autostart = on;
    } catch (e) {
      ui.say(`Cannot change the login setting. ${e instanceof Error ? e.message : String(e)}`, true);
    } finally {
      autostartBusy = false;
    }
  }

  const minutesLabel = (m: number) => (m === 60 ? '1 hour' : m === 1 ? '1 minute' : `${m} minutes`);
</script>

<SettingRow title="Open Chord at login" hint="Chord starts when you log in to this computer.">
  <Toggle
    checked={autostart}
    label="Open Chord at login"
    disabled={autostartBusy}
    onchange={(v) => void setAutostart(v)}
  />
</SettingRow>
<SettingRow title="Start minimised" hint="Chord opens hidden when it starts at login.">
  <Toggle
    checked={prefs.startMinimised}
    label="Start minimised"
    disabled={!autostart}
    onchange={(v) => prefs.set('startMinimised', v)}
  />
</SettingRow>
<SettingRow
  title="Keep running when closed"
  hint={mac ? 'The Dock icon opens Chord again.' : 'The tray icon opens Chord again.'}
>
  <Toggle
    checked={prefs.closeToBackground}
    label="Keep running when closed"
    onchange={(v) => prefs.set('closeToBackground', v)}
  />
</SettingRow>
<SettingRow
  title={mac ? 'Menu bar icon' : 'Tray icon'}
  hint="Shows a dot for unread messages."
>
  <Toggle
    checked={prefs.trayIcon || (!mac && prefs.closeToBackground)}
    label={mac ? 'Menu bar icon' : 'Tray icon'}
    disabled={!mac && prefs.closeToBackground}
    onchange={(v) => prefs.set('trayIcon', v)}
  />
</SettingRow>
<SettingRow title="Remember window size and place" hint="Chord opens where you left it.">
  <Toggle
    checked={prefs.rememberWindow}
    label="Remember window size and place"
    onchange={(v) => prefs.set('rememberWindow', v)}
  />
</SettingRow>
<SettingRow title="Away when idle" hint="Your status returns to Online when you do.">
  <Toggle checked={prefs.autoAway} label="Away when idle" onchange={(v) => prefs.set('autoAway', v)} />
</SettingRow>
{#if prefs.autoAway}
  <SettingRow title="Away after">
    <select
      class="input"
      aria-label="Away after"
      value={prefs.autoAwayMinutes}
      onchange={(e) => prefs.set('autoAwayMinutes', Number(e.currentTarget.value))}
    >
      {#each AUTO_AWAY_MINUTES as m (m)}
        <option value={m}>{minutesLabel(m)}</option>
      {/each}
    </select>
  </SettingRow>
{/if}
