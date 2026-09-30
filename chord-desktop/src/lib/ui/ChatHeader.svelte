<script lang="ts">
  // 48px header: channel name, topic, member list toggle, search.
  import AtSign from 'lucide-svelte/icons/at-sign';
  import Hash from 'lucide-svelte/icons/hash';
  import Users from 'lucide-svelte/icons/users';
  import UserRound from 'lucide-svelte/icons/user-round';
  import Icon from './Icon.svelte';
  import PinsPanel from './PinsPanel.svelte';
  import Presence from './Presence.svelte';
  import SearchPanel from './SearchPanel.svelte';
  import { app } from './app.svelte';
  import { tooltip } from './tooltip';
  import { isGroup, presenceKind } from './types';
  import { ui } from './ui.svelte';

  const c = $derived(app.channel);
  const isDm = $derived(c?.kind === 'dm');
  const group = $derived(!!c && isGroup(c));
  // The rail button: the member list of a room or a group, the profile of a DM peer.
  const railWhat = $derived(app.sideRail === 'profile' ? 'user profile' : 'member list');
</script>

<header class="bar">
  {#if c}
    <span class="ico"><Icon icon={isDm ? AtSign : group ? Users : Hash} size={20} /></span>
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
  {#if app.sideRail}
    <button
      class="icon"
      class:on={ui.membersOpen}
      aria-label={app.sideRail === 'profile' ? 'User profile' : 'Member list'}
      aria-pressed={ui.membersOpen}
      use:tooltip={{ text: `${ui.membersOpen ? 'Hide' : 'Show'} ${railWhat}`, side: 'bottom' }}
      onclick={() => ui.toggleMembers()}
    >
      <Icon icon={app.sideRail === 'profile' ? UserRound : Users} size={20} />
    </button>
  {/if}
  {#if c && !c.pm}<PinsPanel />{/if}
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
