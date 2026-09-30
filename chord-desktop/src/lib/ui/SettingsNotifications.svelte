<script lang="ts">
  // Notifications. The core applies the level of each chat (set in the chat menus). These
  // settings narrow the notice: Rust reads the first and the last from the saved settings (notify.rs), and
  // the sound plays in the page (notices.ts). There is no default level here: the core has
  // no global default, so a setting for it would do nothing.
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { prefs } from './prefs.svelte';
</script>

<SettingRow title="Desktop notifications" hint="Show a system notice when a message arrives and Chord is in the background.">
  <Toggle
    checked={prefs.desktopNotifications}
    label="Desktop notifications"
    onchange={(v) => prefs.set('desktopNotifications', v)}
  />
</SettingRow>
<SettingRow title="Sound" hint="Play a short, quiet sound when a message arrives in a chat you do not read.">
  <Toggle checked={prefs.sound} label="Sound" onchange={(v) => prefs.set('sound', v)} />
</SettingRow>
<SettingRow title="Mute DMs" hint="No notice and no sound from any DM until you turn this off.">
  <Toggle checked={prefs.muteDms} label="Mute DMs" onchange={(v) => prefs.set('muteDms', v)} />
</SettingRow>
