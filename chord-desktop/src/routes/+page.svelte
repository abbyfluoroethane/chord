<script lang="ts">
  // Login or the app, by connection state. ?conn=reconnecting previews the banner.
  import { onMount } from 'svelte';
  import AppShell from '$lib/ui/AppShell.svelte';
  import LoginScreen from '$lib/ui/LoginScreen.svelte';
  import { session } from '$lib/ui/session.svelte';

  onMount(() => {
    const conn = new URLSearchParams(location.search).get('conn');
    if (conn === 'connected' || conn === 'reconnecting') session.force(conn);
  });
</script>

{#if session.state === 'signed-out' || session.state === 'connecting'}
  <LoginScreen />
{:else}
  <AppShell />
{/if}
