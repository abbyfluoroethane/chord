<script lang="ts">
  // 52px panel at the bottom of the sidebar. The avatar opens the status menu. The gear opens settings.
  import Settings from 'lucide-svelte/icons/settings';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import StatusMenu from './StatusMenu.svelte';
  import { app } from './app.svelte';
  import { ownLabel, presenceKind } from './types';
  import { tooltip } from './tooltip';
  import { ui } from './ui.svelte';

  let panel = $state<HTMLDivElement>();
  let open = $state(false);

  const kind = $derived(presenceKind(true, app.me.show));

</script>

<div class="panel" bind:this={panel}>
  <button
    class="who"
    aria-label="Your status: {ownLabel(app.me.show)}"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <Avatar name={app.me.name} src={app.me.avatar} size={32} presence={kind} cut="var(--surface-300)" />
    <span class="text">
      <span class="name">{app.me.name}</span>
      {#if app.me.status}
        <span class="addr" title={app.me.status}>{app.me.status}</span>
      {:else}
        <span class="addr mono" title={ownLabel(app.me.show)}>{app.me.address}</span>
      {/if}
    </span>
  </button>
  <button
    class="icon"
    aria-label="User settings"
    use:tooltip={{ text: 'User settings', side: 'top' }}
    onclick={() => ui.openSettings()}
  >
    <Icon icon={Settings} size={18} />
  </button>
</div>

{#if open && panel}
  <StatusMenu anchor={panel} onclose={() => (open = false)} />
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
    flex: 1;
    min-width: 0;
    height: 40px;
    padding: 0 var(--space-1);
    border-radius: var(--radius-md);
    text-align: left;
  }
  .who:hover {
    background: var(--hover);
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
