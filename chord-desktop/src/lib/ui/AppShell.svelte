<script lang="ts">
  // The signed-in window: rail | sidebar | chat | members. Global shortcuts live here.
  import { onMount, untrack } from 'svelte';
  import AddCircleModal from './AddCircleModal.svelte';
  import ChannelSidebar from './ChannelSidebar.svelte';
  import Chat from './Chat.svelte';
  import CircleDialog from './CircleDialog.svelte';
  import CircleRail from './CircleRail.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import ContextMenuHost from './ContextMenuHost.svelte';
  import ForwardModal from './ForwardModal.svelte';
  import ConnectionBanner from './ConnectionBanner.svelte';
  import ContactsPage from './ContactsPage.svelte';
  import MemberList from './MemberList.svelte';
  import PersonLayer from './PersonLayer.svelte';
  import QuickSwitcher from './QuickSwitcher.svelte';
  import SettingsOverlay from './SettingsOverlay.svelte';
  import ShortcutsModal from './ShortcutsModal.svelte';
  import XmppLinkModal from './XmppLinkModal.svelte';
  import Toast from './Toast.svelte';
  import { app, HOME } from './app.svelte';
  import { api, live } from './bridge';
  import { watchClientState } from './clientstate';
  import { contactsStore } from './contacts.svelte';
  import { drafts } from './drafts.svelte';
  import { prefs } from './prefs.svelte';
  import { rail } from './rail.svelte';
  import { session } from './session.svelte';
  import { spaceKey } from './types';
  import { ui } from './ui.svelte';
  import { xmppLinks } from './xmpplinks.svelte';

  onMount(() => {
    ui.load();
    prefs.load();
    drafts.load();
    app.loadLocal();
    contactsStore.loadLocal();
    // Tell the server when nobody looks at the window (XEP-0352).
    if (live) {
      return watchClientState((active) => {
        void api()
          .then((b) => b.setClientActive(active))
          .catch(() => {
            /* A lost hint does no harm. */
          });
      });
    }
  });

  // Load the rail layout once the spaces are known. Before that the list is empty and
  // the saved folders would look like they lost their spaces.
  let railStarted = false;
  $effect(() => {
    if (!app.spacesReady || railStarted) return;
    railStarted = true;
    const ids = untrack(() => app.spaces.map(spaceKey));
    void rail.init(ids);
  });

  // Keep the rail in step when spaces come and go.
  $effect(() => {
    const ids = app.spaces.map(spaceKey);
    if (app.spacesReady) rail.sync(ids);
  });

  // A link that arrived before the sign-in waits. Ask about it when the session is up.
  $effect(() => {
    if (session.state === 'connected') xmppLinks.flush();
  });

  function keydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      ui.switcherOpen = !ui.switcherOpen;
    } else if (mod && e.key === ',') {
      e.preventDefault();
      if (ui.settingsOpen) ui.settingsOpen = false;
      else ui.openSettings();
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
    {#if app.selectedSpace === HOME && app.showContacts}
      <ContactsPage />
    {:else}
      <Chat />
    {/if}
    {#if ui.membersOpen && app.selectedSpace !== HOME}
      <MemberList />
    {/if}
  </div>
</div>

<PersonLayer />
<ContextMenuHost />
{#if ui.settingsOpen}<SettingsOverlay />{:else}<Toast />{/if}
{#if ui.switcherOpen}<QuickSwitcher />{/if}
{#if ui.shortcutsOpen}<ShortcutsModal onclose={() => (ui.shortcutsOpen = false)} />{/if}
{#if ui.forwarding}
  <ForwardModal item={ui.forwarding} onclose={() => (ui.forwarding = null)} />
{/if}
{#if xmppLinks.asking}
  <XmppLinkModal link={xmppLinks.asking} onclose={() => xmppLinks.dismiss()} />
{/if}
{#if ui.confirm}<ConfirmModal state={ui.confirm} onclose={() => (ui.confirm = null)} />{/if}
{#if ui.circleDialog}
  <CircleDialog
    kind={ui.circleDialog.kind}
    space={ui.circleDialog.space}
    onclose={() => (ui.circleDialog = null)}
  />
{/if}
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
