<script lang="ts">
  import Avatar from './Avatar.svelte';
  import { app } from './app.svelte';
  import { presenceKind, type MemberItem } from './types';
  import { ui } from './ui.svelte';

  let { member }: { member: MemberItem } = $props();

  let btn = $state<HTMLButtonElement>();
  const open = $derived(!!btn && ui.popout?.anchor === btn);
  const isMe = $derived(member.id === app.me.address);
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
    <span class="name" class:me={isMe}>{member.name}</span>
    {#if member.role}<span class="role meta">{member.role}</span>{/if}
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
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    line-height: 18px;
  }
  .name.me {
    color: var(--brand-ink);
  }
  .role {
    line-height: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
