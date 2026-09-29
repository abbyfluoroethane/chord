<script lang="ts">
  // Login or the app, by connection state. ?conn=reconnecting previews the banner.
  // Inside Tauri the session boots first: it reads the settings and, if a password is
  // saved, logs in by itself.
  import { onMount } from 'svelte';
  import AppShell from '$lib/ui/AppShell.svelte';
  import LoginScreen from '$lib/ui/LoginScreen.svelte';
  import { session } from '$lib/ui/session.svelte';

  onMount(() => {
    void session.boot();
    const conn = new URLSearchParams(location.search).get('conn');
    if (conn === 'connected' || conn === 'reconnecting') session.force(conn);
  });
</script>

{#if session.booting}
  <main class="boot" aria-busy="true"></main>
{:else if session.state === 'signed-out' || session.state === 'connecting'}
  <LoginScreen />
{:else}
  <AppShell />
{/if}

<style>
  .boot {
    min-height: 100vh;
    background: var(--surface-100);
  }
</style>
