<script lang="ts">
  // The app shell with the sample data. #login shows the login screen, and
  // #reconnecting shows the reconnect banner. #contacts opens the contacts page,
  // #settings opens user settings (add /appearance, /privacy and so on for a page),
  // and #profile opens the full profile of a contact. #omemo opens the encrypted chat with
  // Sam, #omemo/devices adds the devices dialog, #omemo/notice adds the new key notice,
  // and #settings/encryption opens the Encryption page (add /reset for the keys reset notice). Only a plain #anchor reaches a shared page.
  import { onMount } from 'svelte';
  import AppShell from '$lib/ui/AppShell.svelte';
  import LoginScreen from '$lib/ui/LoginScreen.svelte';
  import { app } from '$lib/ui/app.svelte';
  import { session } from '$lib/ui/session.svelte';
  import type { SettingsPage } from '$lib/ui/types';
  import { ui } from '$lib/ui/ui.svelte';
  import { prefs } from '$lib/ui/prefs.svelte';
  import { theme } from '$lib/theme/theme.svelte';
  import { omemo } from '$lib/ui/omemo.svelte';

  function apply() {
    const [view, sub] = location.hash.slice(1).split('/');
    if (view === 'login') return;
    session.force(view === 'reconnecting' ? 'reconnecting' : 'connected');
    ui.settingsOpen = view === 'settings';
    omemo.devicesFor = view === 'omemo' && sub === 'devices' ? 'sam@other.example' : null;
    omemo.notices =
      view === 'omemo' && sub === 'notice'
        ? [{ jid: 'sam@other.example', deviceId: 772901, fingerprint: '' }]
        : [];
    omemo.keysReset = view === 'settings' && sub === 'encryption' && location.hash.endsWith('/reset');
    if (view === 'omemo') app.selectChannel('sam@other.example');
    ui.profile = view === 'profile' ? (sub ? `${sub}@foid.space` : 'rin@foid.space') : null;
    if (view === 'contacts') app.openContacts();
    if (view === 'settings') {
      const pages: SettingsPage[] = ['account', 'privacy', 'encryption', 'notifications', 'appearance', 'keybinds', 'about'];
      ui.settingsPage = pages.find((p) => p === sub) ?? 'account';
    }
  }

  onMount(() => {
    theme.load();
    prefs.load();
    apply();
    window.addEventListener('hashchange', apply);
    return () => window.removeEventListener('hashchange', apply);
  });
</script>

{#if session.state === 'signed-out' || session.state === 'connecting'}
  <LoginScreen />
{:else}
  <AppShell />
{/if}
