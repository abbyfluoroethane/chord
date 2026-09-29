<script lang="ts">
  import Hash from 'lucide-svelte/icons/hash';
  import BellOff from 'lucide-svelte/icons/bell-off';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import { presenceKind, type ChannelItem } from './types';

  let {
    channel,
    selected,
    onclick
  }: { channel: ChannelItem; selected: boolean; onclick: () => void } = $props();

  const isDm = $derived(channel.kind === 'dm');
  const unread = $derived(channel.unread > 0 && !channel.muted);
  const count = $derived(isDm ? channel.unread : channel.mentions);
  const desc = $derived(
    `${isDm ? '' : '#'}${channel.name}` +
      (channel.muted ? ', muted' : '') +
      (count ? `, ${count} ${isDm ? 'unread' : count === 1 ? 'mention' : 'mentions'}` : unread ? ', unread' : '')
  );
</script>

<div class="wrap">
  {#if unread && !selected}<span class="pill"></span>{/if}
  <button
    class="row"
    class:selected
    class:unread
    class:muted={channel.muted}
    class:dm={isDm}
    class:offline={isDm && !channel.online}
    aria-label={desc}
    aria-current={selected ? 'true' : undefined}
    {onclick}
  >
    {#if isDm}
      <Avatar
        name={channel.name}
        src={channel.avatar}
        size={32}
        presence={presenceKind(channel.online, channel.show)}
        cut={selected ? 'var(--surface-300)' : 'var(--surface-200)'}
      />
    {:else}
      <span class="hash"><Icon icon={Hash} size={18} /></span>
    {/if}
    <span class="name">{channel.name}</span>
    {#if channel.muted}
      <span class="hash"><Icon icon={BellOff} size={14} /></span>
    {:else if count > 0}
      <span class="badge" aria-hidden="true">{count > 99 ? '99+' : count}</span>
    {/if}
  </button>
</div>

<style>
  .wrap {
    position: relative;
    padding: 1px var(--space-2);
  }
  .pill {
    position: absolute;
    left: 0;
    top: 50%;
    width: 4px;
    height: 8px;
    transform: translateY(-50%);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    background: var(--ink);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    text-align: left;
    font-weight: 500;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .row.dm {
    height: 42px;
  }
  .row:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .row.selected {
    background: var(--selected);
    color: var(--ink);
  }
  .row.unread {
    color: var(--ink);
    font-weight: 600;
  }
  .row.muted {
    opacity: 0.55;
  }
  .row.dm.offline .name {
    opacity: 0.7;
  }
  .hash {
    display: grid;
    flex: none;
    color: var(--ink-muted);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .badge {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--brand);
    color: var(--on-brand);
    font-size: 11px;
    font-weight: 600;
    line-height: 18px;
    text-align: center;
  }
</style>
