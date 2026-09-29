<script lang="ts">
  // 52px panel at the bottom of the sidebar: avatar with presence, name in amber, address, settings.
  import Settings from 'lucide-svelte/icons/settings';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import Menu, { type MenuItem } from './Menu.svelte';
  import { app } from './app.svelte';
  import { session } from './session.svelte';
  import { presenceKind, presenceLabel, type Show } from './types';
  import { tooltip } from './tooltip';
  import { theme, type ThemeChoice } from '$lib/theme/theme.svelte';
  import LogOut from 'lucide-svelte/icons/log-out';

  let btn = $state<HTMLButtonElement>();
  let open = $state(false);

  const kind = $derived(presenceKind(true, app.me.show));

  const status = (label: string, show: Show): MenuItem => ({
    label,
    checked: app.me.show === show,
    onselect: () => app.setShow(show)
  });
  const themed = (label: string, v: ThemeChoice): MenuItem => ({
    label: `Theme: ${label}`,
    checked: theme.choice === v,
    onselect: () => theme.set(v)
  });

  const items = $derived<MenuItem[]>([
    status('Available', 'chat'),
    status('Away', 'away'),
    status('Do not disturb', 'dnd'),
    { ...themed('system', 'system'), separator: true },
    themed('dark', 'dark'),
    themed('light', 'light'),
    { label: 'Sign out', icon: LogOut, separator: true, danger: true, onselect: () => session.signOut() }
  ]);
</script>

<div class="panel">
  <div class="who">
    <Avatar name={app.me.name} src={app.me.avatar} size={32} presence={kind} cut="var(--surface-300)" />
    <div class="text">
      <span class="name">{app.me.name}</span>
      <span class="addr mono" title={presenceLabel[kind]}>{app.me.address}</span>
    </div>
  </div>
  <button
    bind:this={btn}
    class="icon"
    aria-label="Settings and status"
    aria-haspopup="menu"
    aria-expanded={open}
    use:tooltip={{ text: 'Settings', side: 'top' }}
    onclick={() => (open = !open)}
  >
    <Icon icon={Settings} size={18} />
  </button>
</div>

{#if open && btn}
  <Menu anchor={btn} {items} placement="top-end" label="Settings and status" onclose={() => (open = false)} />
{/if}

<style>
  .panel {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    height: var(--panel-height);
    flex: none;
    padding: 0 var(--space-2);
    background: var(--surface-300);
    border-top: 1px solid var(--line);
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    padding: 0 var(--space-1);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    color: var(--brand-ink);
    font-weight: 600;
    font-size: 14px;
    line-height: 18px;
  }
  .addr {
    font-size: 12px;
    line-height: 16px;
    color: var(--ink-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .icon:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
