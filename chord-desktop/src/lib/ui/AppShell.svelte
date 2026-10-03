<script lang="ts">
  // The signed-in window: rail | sidebar | chat | members. Global shortcuts live here.
  import { onMount, untrack } from 'svelte';
  import AddCircleModal from './AddCircleModal.svelte';
  import ChannelSidebar from './ChannelSidebar.svelte';
  import Chat from './Chat.svelte';
  import CircleDialog from './CircleDialog.svelte';
  import CircleRail from './CircleRail.svelte';
  import ConfirmModal from './ConfirmModal.svelte';
  import CreateRoomModal from './CreateRoomModal.svelte';
  import RoomCaptchaModal from './RoomCaptchaModal.svelte';
  import ContextMenuHost from './ContextMenuHost.svelte';
  import ForwardModal from './ForwardModal.svelte';
  import ConnectionBanner from './ConnectionBanner.svelte';
  import ContactsPage from './ContactsPage.svelte';
  import DmProfile from './DmProfile.svelte';
  import MemberList from './MemberList.svelte';
  import PersonLayer from './PersonLayer.svelte';
  import QuickSwitcher from './QuickSwitcher.svelte';
  import SettingsOverlay from './SettingsOverlay.svelte';
  import ShortcutsModal from './ShortcutsModal.svelte';
  import PasswordModal from './PasswordModal.svelte';
  import RoomAlertCards from './RoomAlertCards.svelte';
  import XmppLinkModal from './XmppLinkModal.svelte';
  import ExternalLinkModal from './ExternalLinkModal.svelte';
  import { leaving } from './leaving.svelte';
  import Toast from './Toast.svelte';
  import { app, HOME } from './app.svelte';
  import { api, live } from './bridge';
  import { watchClientState } from './clientstate';
  import { contactsStore } from './contacts.svelte';
  import { drafts } from './drafts.svelte';
  import { watchIdle } from './idle';
  import { prefs } from './prefs.svelte';
  import { rail } from './rail.svelte';
  import { session } from './session.svelte';
  import { spaceKey } from './types';
  import { pageTitle } from './unread';
  import { handleKey } from './keyactions';
  import { ui } from './ui.svelte';
  import { xmppLinks } from './xmpplinks.svelte';

  onMount(() => {
    ui.load();
    prefs.load();
    drafts.load();
    app.loadLocal();
    // Tell the server when nobody looks at the window (XEP-0352).
    if (live) {
      // Tell the contacts when nobody used Chord for a while (XEP-0319).
      idleWatch = watchIdle((since) => {
        void api()
          .then((b) => b.setIdle(since === null ? null : Math.floor(since / 1000)))
          .catch(() => {
            /* Offline: resend after the next connect. */
          });
      });
      const stopState = watchClientState((active) => {
        void api()
          .then((b) => b.setClientActive(active))
          .catch(() => {
            /* A lost hint does no harm. */
          });
      });
      return () => {
        stopState();
        idleWatch?.stop();
      };
    }
  });

  let idleWatch: ReturnType<typeof watchIdle> | undefined;
  // A new session starts with no idle time: send it again.
  $effect(() => {
    if (session.state === 'connected') idleWatch?.resend();
  });

  // The total of unread messages goes to the window title and the dock badge.
  $effect(() => {
    const n = app.totalUnread;
    document.title = pageTitle(n);
    if (live) {
      void api()
        .then((b) => b.setUnreadCount(n))
        .catch(() => {
          /* A badge that fails does no harm. */
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

  // The keymap decides what each key does. See keymap.ts and keyactions.ts.
  function keydown(e: KeyboardEvent) {
    handleKey(e);
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
    {#if ui.membersOpen && app.sideRail === 'members'}
      <MemberList />
    {:else if ui.membersOpen && app.sideRail === 'profile' && app.channel}
      <DmProfile address={app.channel.jid} name={app.channel.name} />
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
{#if leaving.asking}
  <ExternalLinkModal href={leaving.asking.href} text={leaving.asking.text} onclose={() => leaving.dismiss()} />
{/if}
<RoomAlertCards />
{#if ui.passwordAsk}
  <PasswordModal ask={ui.passwordAsk} onclose={() => (ui.passwordAsk = null)} />
{/if}
{#if ui.createRoomAsk}
  <CreateRoomModal ask={ui.createRoomAsk} onclose={() => (ui.createRoomAsk = null)} />
{/if}
{#if ui.captchaAsk}
  <RoomCaptchaModal ask={ui.captchaAsk} onclose={() => (ui.captchaAsk = null)} />
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
