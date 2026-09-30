<script lang="ts">
  // 48px header: channel name, topic, member list toggle, search.
  import AtSign from 'lucide-svelte/icons/at-sign';
  import Hash from 'lucide-svelte/icons/hash';
  import Users from 'lucide-svelte/icons/users';
  import Icon from './Icon.svelte';
  import Presence from './Presence.svelte';
  import SearchPanel from './SearchPanel.svelte';
  import { app, HOME } from './app.svelte';
  import { tooltip } from './tooltip';
  import { presenceKind } from './types';
  import { ui } from './ui.svelte';

  const c = $derived(app.channel);
  const isDm = $derived(c?.kind === 'dm');
</script>

<header class="bar">
  {#if c}
    <span class="ico"><Icon icon={isDm ? AtSign : Hash} size={20} /></span>
    <h1 class="title">{c.name}</h1>
    {#if isDm && c.unknownPresence !== true}
      <Presence kind={presenceKind(c.online, c.show)} size={10} />
    {/if}
    {#if c.topic}
      <span class="divider" aria-hidden="true"></span>
      <p class="topic" title={c.topic}>{c.topic}</p>
    {/if}
  {/if}
  <span class="spacer"></span>
  {#if app.selectedSpace !== HOME}
    <button
      class="icon"
      class:on={ui.membersOpen}
      aria-label="Member list"
      aria-pressed={ui.membersOpen}
      use:tooltip={{ text: ui.membersOpen ? 'Hide member list' : 'Show member list', side: 'bottom' }}
      onclick={() => ui.toggleMembers()}
    >
      <Icon icon={Users} size={20} />
    </button>
  {/if}
  <SearchPanel />
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--bar-height);
    flex: none;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--line);
    background: var(--surface-100);
  }
  .ico {
    display: grid;
    color: var(--ink-muted);
  }
  h1 {
    margin: 0;
    white-space: nowrap;
  }
  .divider {
    width: 1px;
    height: 24px;
    margin: 0 var(--space-2);
    background: var(--line);
  }
  .topic {
    margin: 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-muted);
    font-size: 14px;
  }
  .spacer {
    flex: 1;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .icon:hover,
  .icon.on {
    color: var(--ink);
  }
  .icon:hover {
    background: var(--hover);
  }
</style>
