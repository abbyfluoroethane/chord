<script lang="ts">
  // A row of the member list: the name, a shield for a moderator, and the status text.
  import Shield from 'lucide-svelte/icons/shield';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { tooltip } from './tooltip';
  import { presenceKind, type MemberItem } from './types';
  import { ui } from './ui.svelte';

  let { member }: { member: MemberItem } = $props();

  let btn = $state<HTMLButtonElement>();
  const open = $derived(!!btn && ui.popout?.anchor === btn);
  const isMe = $derived(member.id === app.me.address);
  const moderator = $derived(member.role === 'Moderator');
</script>

<button
  bind:this={btn}
  class="row"
  class:offline={!member.online}
  aria-haspopup="dialog"
  aria-expanded={open}
  onclick={() => btn && ui.openPopout(member.id, btn, member.name, 'left-start')}
  oncontextmenu={(e) => ui.openPersonMenu(e, member.id, member.name)}
>
  <Avatar
    name={member.name}
    src={member.avatar}
    size={32}
    presence={presenceKind(member.online, member.show)}
    cut="var(--surface-200)"
  />
  <span class="text">
    <span class="line">
      <span class="name" class:me={isMe}>{member.name}</span>
      {#if moderator}
        <span
          class="mod"
          role="img"
          aria-label="Moderator"
          use:tooltip={{ text: 'Moderator', side: 'top' }}
        >
          <Icon icon={Shield} size={13} />
        </span>
      {/if}
    </span>
    {#if member.status}<span class="status meta">{member.status}</span>{/if}
  </span>
</button>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: calc(100% - var(--space-4));
    height: 42px;
    margin: 0 var(--space-2);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    text-align: left;
    transition: background var(--dur-fast);
  }
  .row:hover,
  .row[aria-expanded='true'] {
    background: var(--hover);
  }
  .row.offline {
    opacity: 0.55;
  }
  .row.offline:hover {
    opacity: 1;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    min-width: 0;
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    line-height: 18px;
  }
  .name.me {
    color: var(--brand-ink);
  }
  .mod {
    display: inline-flex;
    flex: none;
    color: var(--ink-muted);
  }
  .status {
    line-height: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
