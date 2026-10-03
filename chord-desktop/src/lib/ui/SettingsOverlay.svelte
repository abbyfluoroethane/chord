<script lang="ts">
  // Full-screen user settings, as in Discord 2019. A native dialog gives the focus
  // trap, Esc, and focus return. Ctrl+, opens it (see AppShell).
  import { untrack } from 'svelte';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import LogoutModal from './LogoutModal.svelte';
  import SettingsAbout from './SettingsAbout.svelte';
  import SettingsAccount from './SettingsAccount.svelte';
  import SettingsAppearance from './SettingsAppearance.svelte';
  import SettingsChat from './SettingsChat.svelte';
  import SettingsBehaviour from './SettingsBehaviour.svelte';
  import SettingsKeybinds from './SettingsKeybinds.svelte';
  import SettingsNav from './SettingsNav.svelte';
  import SettingsNotifications from './SettingsNotifications.svelte';
  import SettingsPrivacy from './SettingsPrivacy.svelte';
  import SettingsAdvanced from './SettingsAdvanced.svelte';
  import Toast from './Toast.svelte';
  import { ui } from './ui.svelte';

  let dlg = $state<HTMLDialogElement>();

  const titles = {
    account: 'My account',
    privacy: 'Privacy',
    notifications: 'Notifications',
    behaviour: 'App behaviour',
    appearance: 'Appearance',
    chat: 'Chat',
    keybinds: 'Keybinds',
    advanced: 'Advanced',
    about: 'About'
  } as const;

  function close() {
    ui.settingsOpen = false;
  }

  $effect(() => {
    if (!dlg) return;
    const opener = document.activeElement as HTMLElement | null;
    dlg.showModal();
    untrack(() => (ui.overlays += 1));
    return () => {
      untrack(() => (ui.overlays -= 1));
      if (opener?.isConnected) opener.focus?.();
    };
  });

  function cancel(e: Event) {
    e.preventDefault();
    close();
  }
</script>

<dialog bind:this={dlg} aria-label="User settings" oncancel={cancel}>
  <div class="frame">
    <div class="navcol"><SettingsNav onlogout={() => (ui.logoutOpen = true)} /></div>
    <div class="pagecol">
      <div class="page">
        <h1 class="page-title">{titles[ui.settingsPage]}</h1>
        {#if ui.settingsPage === 'account'}
          <SettingsAccount />
        {:else if ui.settingsPage === 'privacy'}
          <SettingsPrivacy />
        {:else if ui.settingsPage === 'notifications'}
          <SettingsNotifications />
        {:else if ui.settingsPage === 'behaviour'}
          <SettingsBehaviour />
        {:else if ui.settingsPage === 'appearance'}
          <SettingsAppearance />
        {:else if ui.settingsPage === 'chat'}
          <SettingsChat />
        {:else if ui.settingsPage === 'keybinds'}
          <SettingsKeybinds />
        {:else if ui.settingsPage === 'advanced'}
          <SettingsAdvanced />
        {:else}
          <SettingsAbout />
        {/if}
      </div>
      <div class="closer">
        <button class="x" aria-label="Close settings" title="Close settings" onclick={close}><Icon icon={X} size={18} /></button>
        <span class="esc" aria-hidden="true">ESC</span>
      </div>
    </div>
  </div>

  <Toast />
  {#if ui.logoutOpen}<LogoutModal onclose={() => (ui.logoutOpen = false)} />{/if}
</dialog>

<style>
  dialog {
    width: 100vw;
    height: 100vh;
    max-width: none;
    max-height: none;
    margin: 0;
    padding: 0;
    border: 0;
    color: var(--ink);
    background: var(--surface-100);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  dialog[open] {
    display: block;
  }
  dialog::backdrop {
    background: var(--surface-100);
  }
  .frame {
    display: flex;
    height: 100%;
  }
  .navcol {
    display: flex;
    flex: 1 0 auto;
    justify-content: flex-end;
    min-width: 0;
    background: var(--surface-200);
    overflow-y: auto;
  }
  .pagecol {
    position: relative;
    display: flex;
    flex: 1 1 800px;
    min-width: 0;
    overflow-y: auto;
  }
  .page {
    width: 100%;
    max-width: 740px;
    padding: 60px 40px 80px;
  }
  .page-title {
    margin: 0 0 var(--space-6);
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 24px;
    line-height: 30px;
    letter-spacing: -0.02em;
  }
  .closer {
    position: sticky;
    top: 60px;
    align-self: flex-start;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    margin: 60px 0 0 var(--space-2);
    flex: none;
  }
  .x {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border: 2px solid var(--ink-muted);
    border-radius: 50%;
    color: var(--ink-muted);
    transition:
      color var(--dur-fast),
      border-color var(--dur-fast),
      background var(--dur-fast);
  }
  .x:hover {
    color: var(--ink);
    border-color: var(--ink);
    background: var(--hover);
  }
  .esc {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    color: var(--ink-muted);
  }
</style>
