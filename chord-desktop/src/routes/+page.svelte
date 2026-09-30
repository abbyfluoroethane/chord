<script lang="ts">
  // Splash, login or the app, by connection state. ?conn=reconnecting previews the banner.
  // Inside Tauri the session boots first: it reads the settings and, if a password is
  // saved, logs in by itself. The splash shows during that time, never the login form.
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import AppShell from '$lib/ui/AppShell.svelte';
  import LoginScreen from '$lib/ui/LoginScreen.svelte';
  import Splash from '$lib/ui/Splash.svelte';
  import { session } from '$lib/ui/session.svelte';

  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;

  onMount(() => {
    // The Svelte splash or the app is on screen now. Remove the inline splash of app.html.
    document.getElementById('boot-splash')?.remove();
    void session.boot();
    const conn = new URLSearchParams(location.search).get('conn');
    if (conn === 'connected' || conn === 'reconnecting') session.force(conn);
  });
</script>

{#if session.splash}
  <div out:fade={{ duration: reduced() ? 0 : 240 }}>
    <Splash
      status={session.booting ? 'Starting Chord' : `Connecting to ${session.host}`}
      onCancel={session.booting ? undefined : () => void session.cancelRestore()}
    />
  </div>
{:else if session.state === 'signed-out' || session.state === 'connecting'}
  <LoginScreen />
{:else}
  <AppShell />
{/if}
