<script lang="ts">
  // Notifications. Levels map to api.setNotificationLevel per chat or room. These are the defaults.
  import SettingRow from './SettingRow.svelte';
  import Segmented from './Segmented.svelte';
  import Toggle from './Toggle.svelte';
  import { prefs } from './prefs.svelte';
  import type { NotificationLevel } from './types';

  const levels: { value: NotificationLevel; label: string }[] = [
    { value: 'all', label: 'All' },
    { value: 'mentions', label: 'Mentions' },
    { value: 'nothing', label: 'Nothing' }
  ];
</script>

<SettingRow title="Desktop notifications" hint="Show a notice when a message arrives.">
  <Toggle
    checked={prefs.desktopNotifications}
    label="Desktop notifications"
    onchange={(v) => prefs.set('desktopNotifications', v)}
  />
</SettingRow>
<SettingRow title="Sound" hint="Play a sound with each notice.">
  <Toggle checked={prefs.sound} label="Sound" onchange={(v) => prefs.set('sound', v)} />
</SettingRow>
<SettingRow title="Default level for DMs" hint="You can change it for each DM.">
  <Segmented
    label="Default level for DMs"
    value={prefs.dmLevel}
    options={levels}
    onchange={(v) => prefs.set('dmLevel', v)}
  />
</SettingRow>
<SettingRow title="Default level for channels" hint="You can change it for each channel.">
  <Segmented
    label="Default level for channels"
    value={prefs.channelLevel}
    options={levels}
    onchange={(v) => prefs.set('channelLevel', v)}
  />
</SettingRow>
<SettingRow title="Mute DMs" hint="No notices from any DM until you turn this off.">
  <Toggle checked={prefs.muteDms} label="Mute DMs" onchange={(v) => prefs.set('muteDms', v)} />
</SettingRow>
