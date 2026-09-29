<script lang="ts">
  // The signed-in window: rail | sidebar | chat | members. Global shortcuts live here.
  import { onMount } from 'svelte';
  import AddCircleModal from './AddCircleModal.svelte';
  import ChannelSidebar from './ChannelSidebar.svelte';
  import Chat from './Chat.svelte';
  import CircleRail from './CircleRail.svelte';
  import ConnectionBanner from './ConnectionBanner.svelte';
  import MemberList from './MemberList.svelte';
  import QuickSwitcher from './QuickSwitcher.svelte';
  import ShortcutsModal from './ShortcutsModal.svelte';
  import { app, HOME } from './app.svelte';
  import { rail } from './rail.svelte';
  import { spaceKey } from './types';
  import { ui } from './ui.svelte';

  onMount(() => {
    ui.load();
    void rail.init(app.spaces.map(spaceKey));
  });

  // Keep the rail in step when circles come and go.
  $effect(() => {
    const ids = app.spaces.map(spaceKey);
    rail.sync(ids);
  });

  function keydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      ui.switcherOpen = !ui.switcherOpen;
    } else if (mod && e.key === '/') {
      e.preventDefault();
      ui.shortcutsOpen = !ui.shortcutsOpen;
    } else if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      const dir = e.key === 'ArrowUp' ? -1 : 1;
      if (e.shiftKey) app.stepUnread(dir);
      else app.step(dir);
    } else if (e.key === 'Escape' && !e.defaultPrevented && ui.overlays === 0 && !app.editingId) {
      app.markRead();
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="app">
  <ConnectionBanner />
  <div class="cols">
    <CircleRail />
    <ChannelSidebar />
    <Chat />
    {#if ui.membersOpen && app.selectedSpace !== HOME}
      <MemberList />
    {/if}
  </div>
</div>

{#if ui.switcherOpen}<QuickSwitcher />{/if}
{#if ui.shortcutsOpen}<ShortcutsModal onclose={() => (ui.shortcutsOpen = false)} />{/if}
{#if ui.addCircleOpen}<AddCircleModal onclose={() => (ui.addCircleOpen = false)} />{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .cols {
    display: flex;
    flex: 1;
    min-height: 0;
  }
</style>
