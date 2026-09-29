<script lang="ts">
  // 48px header of the channel sidebar. Opens the circle menu.
  import Bell from 'lucide-svelte/icons/bell';
  import ChevronDown from 'lucide-svelte/icons/chevron-down';
  import LogOut from 'lucide-svelte/icons/log-out';
  import Pencil from 'lucide-svelte/icons/pencil';
  import Plus from 'lucide-svelte/icons/plus';
  import Settings from 'lucide-svelte/icons/settings';
  import UserPlus from 'lucide-svelte/icons/user-plus';
  import CircleDialog, { type DialogKind } from './CircleDialog.svelte';
  import Icon from './Icon.svelte';
  import Menu, { type MenuItem } from './Menu.svelte';

  let { name, space }: { name: string; space: string } = $props();

  let btn = $state<HTMLButtonElement>();
  let menuOpen = $state(false);
  let dialog = $state<DialogKind | null>(null);

  const items: MenuItem[] = [
    { label: 'Invite people', icon: UserPlus, onselect: () => (dialog = 'invite') },
    { label: 'Circle settings', icon: Settings, onselect: () => (dialog = 'settings') },
    { label: 'Create channel', icon: Plus, onselect: () => (dialog = 'create-channel') },
    { label: 'Notification settings', icon: Bell, separator: true, onselect: () => (dialog = 'notifications') },
    { label: 'Change nickname', icon: Pencil, onselect: () => (dialog = 'nickname') },
    { label: 'Leave circle', icon: LogOut, danger: true, separator: true, onselect: () => (dialog = 'leave') }
  ];
</script>

<button
  bind:this={btn}
  class="header"
  aria-haspopup="menu"
  aria-expanded={menuOpen}
  onclick={() => (menuOpen = !menuOpen)}
>
  <span class="name">{name}</span>
  <Icon icon={ChevronDown} size={18} />
</button>

{#if menuOpen && btn}
  <Menu anchor={btn} {items} label="{name} menu" onclose={() => (menuOpen = false)} />
{/if}
{#if dialog}
  <CircleDialog kind={dialog} {space} onclose={() => (dialog = null)} />
{/if}

<style>
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    height: var(--bar-height);
    flex: none;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--line);
    font-weight: 600;
    font-size: 15px;
    text-align: left;
    transition: background var(--dur-fast);
  }
  .header:hover,
  .header[aria-expanded='true'] {
    background: var(--hover);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
