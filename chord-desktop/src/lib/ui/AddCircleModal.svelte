<script lang="ts">
  // "Create a circle" and "Join a circle". Bridge calls: createSpace(name, access),
  // browseSpaces, joinSpace, and pendingSpaceJoins.
  import type { SpaceAccess } from '$lib/chord/types';
  import Search from 'lucide-svelte/icons/search';
  import CircleIcon from './CircleIcon.svelte';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { live } from './bridge';
  import { spaceKey } from './types';

  let { onclose }: { onclose: () => void } = $props();

  let tab = $state<'create' | 'join'>('create');
  let name = $state('');
  let query = $state('');
  let access = $state<SpaceAccess>('open');
  let busy = $state(false);

  // The list of public circles comes from the server.
  $effect(() => {
    if (tab === 'join') {
      void app.loadPublicCircles();
      void app.loadPendingJoins();
    }
  });

  const joined = $derived(new Set(app.spaces.map(spaceKey)));
  const shown = $derived(
    app.publicCircles.filter((c) => c.name.toLowerCase().includes(query.trim().toLowerCase()))
  );

  async function create(e: SubmitEvent) {
    e.preventDefault();
    if (!name.trim() || busy) return;
    busy = true;
    const ok = await app.createCircleAsync(name, access);
    busy = false;
    if (ok) onclose();
  }

  async function join(c: (typeof shown)[number]) {
    busy = true;
    const ok = await app.joinCircleAsync(c);
    busy = false;
    if (ok) onclose();
  }
</script>

<Modal title={tab === 'create' ? 'Create a circle' : 'Join a circle'} {onclose} size="medium">
  <div class="tabs" role="tablist" aria-label="Add a circle">
    <button role="tab" aria-selected={tab === 'create'} class:on={tab === 'create'} onclick={() => (tab = 'create')}>
      Create a circle
    </button>
    <button role="tab" aria-selected={tab === 'join'} class:on={tab === 'join'} onclick={() => (tab = 'join')}>
      Join a circle
    </button>
  </div>

  {#if tab === 'create'}
    <form onsubmit={create} class="form">
      <p class="hint">Anyone can make a circle. It lives on your server and you own it.</p>
      <div class="field">
        <label for="circle-name">Circle name</label>
        <input id="circle-name" class="input" bind:value={name} placeholder="Launch Ops" autocomplete="off" />
      </div>
      {#if live}
        <div class="field">
          <label for="circle-access">Who can join</label>
          <select id="circle-access" class="input" bind:value={access}>
            <option value="open">Anyone</option>
            <option value="authorize">People I approve</option>
            <option value="whitelist">Only people I invite</option>
          </select>
        </div>
      {/if}
      <button class="btn btn-primary" type="submit" disabled={!name.trim() || busy}>Create circle</button>
    </form>
  {:else}
    <div class="form">
      <div class="search">
        <Icon icon={Search} size={16} />
        <input
          class="bare"
          aria-label="Search public circles"
          placeholder="Search public circles"
          bind:value={query}
        />
      </div>
      <ul class="list">
        {#each shown as c (spaceKey(c))}
          <li>
            <span class="ico"><CircleIcon name={c.name} /></span>
            <span class="info">
              <span class="name">{c.name}</span>
              <span class="meta">{c.description}</span>
              <span class="mono addr"
                >{spaceKey(c)}{c.members === null ? '' : ` · ${c.members} members`}</span
              >
            </span>
            {#if joined.has(spaceKey(c))}
              <span class="meta">Joined</span>
            {:else}
              <button
                class="btn"
                aria-label="Join {c.name}"
                disabled={busy}
                onclick={() => void join(c)}>Join</button
              >
            {/if}
          </li>
        {:else}
          <li class="empty">No circle matches. Try an address instead.</li>
        {/each}
      </ul>
      {#if live && app.pendingJoins.length}
        <div class="pending">
          <span class="meta">Waiting for approval</span>
          {#each app.pendingJoins as p (p.service + '/' + p.node)}
            <span class="mono addr">{p.name} · {p.service}/{p.node}</span>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</Modal>

<style>
  .tabs {
    display: flex;
    gap: var(--space-1);
    margin-bottom: var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    padding: var(--space-2) var(--space-3);
    color: var(--ink-muted);
    font-weight: 500;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color var(--dur-fast);
  }
  .tabs button:hover {
    color: var(--ink);
  }
  .tabs button.on {
    color: var(--ink);
    border-bottom-color: var(--brand);
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .hint {
    margin: 0;
    color: var(--ink-muted);
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 40px;
    padding: 0 var(--space-3);
    border: 1px solid var(--line);
    background: var(--surface-300);
    border-radius: var(--radius-md);
    color: var(--ink-muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  .bare {
    flex: 1;
    background: none;
    border: 0;
    outline: 0;
    color: var(--ink);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-100);
  }
  .ico {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: var(--radius-circle-icon);
    overflow: hidden;
    font-weight: 600;
    font-size: 14px;
  }
  .info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .addr {
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
  }
  .pending {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .empty {
    color: var(--ink-muted);
    justify-content: center;
  }
</style>
