<script lang="ts">
  // The app shell with the sample data. #login shows the login screen, and
  // #reconnecting shows the reconnect banner. Only a plain #anchor reaches a shared page.
  import { onMount } from 'svelte';
  import AppShell from '$lib/ui/AppShell.svelte';
  import LoginScreen from '$lib/ui/LoginScreen.svelte';
  import { session } from '$lib/ui/session.svelte';
  import { theme } from '$lib/theme/theme.svelte';

  function apply() {
    const view = location.hash.slice(1);
    if (view === 'login') return;
    session.force(view === 'reconnecting' ? 'reconnecting' : 'connected');
  }

  onMount(() => {
    theme.load();
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
