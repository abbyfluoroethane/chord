<script lang="ts">
  // Small profile card for a member.
  import Avatar from './Avatar.svelte';
  import Popover from './Popover.svelte';
  import { app } from './app.svelte';
  import { presenceKind, presenceLabel, type MemberItem } from './types';

  let {
    member,
    anchor,
    onclose
  }: { member: MemberItem; anchor: HTMLElement; onclose: () => void } = $props();

  const kind = $derived(presenceKind(member.online, member.show));
  const isMe = $derived(member.id === app.me.address);
</script>

<Popover {anchor} {onclose} placement="left-start" label="Profile of {member.name}">
  <div class="card">
    <div class="banner"></div>
    <div class="avatar"><Avatar name={member.name} src={member.avatar} size={64} presence={kind} cut="var(--surface-300)" /></div>
    <div class="body">
      <h3 class="title" class:me={isMe}>{member.name}</h3>
      <p class="addr mono">{member.id}</p>
      <hr />
      <dl>
        <dt class="field-label">Status</dt>
        <dd>{presenceLabel[kind]}</dd>
        <dt class="field-label">Role</dt>
        <dd>{member.role ?? (member.affiliation === 'none' ? 'Guest' : member.affiliation[0].toUpperCase() + member.affiliation.slice(1))}</dd>
      </dl>
      {#if !isMe}
        <button
          class="btn btn-primary"
          onclick={() => {
            onclose();
            app.openDm(member.id, member.name);
          }}>Send message</button
        >
      {/if}
    </div>
  </div>
</Popover>

<style>
  .card {
    position: relative;
    width: 280px;
  }
  .banner {
    height: 56px;
    background: var(--brand-soft);
    border-bottom: 1px solid var(--line);
  }
  .avatar {
    position: absolute;
    top: 24px;
    left: var(--space-4);
    padding: 3px;
    border-radius: 50%;
    background: var(--surface-300);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: 44px var(--space-4) var(--space-4);
  }
  h3 {
    margin: 0;
  }
  h3.me {
    color: var(--brand-ink);
  }
  .addr {
    margin: 0;
    color: var(--ink-muted);
    overflow-wrap: anywhere;
  }
  hr {
    width: 100%;
    margin: var(--space-1) 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
  dl {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  dd {
    margin: 0 0 var(--space-2);
  }
</style>
