<script lang="ts">
  // "Active now": the right column of the contacts page. Online contacts and their status text.
  import Avatar from './Avatar.svelte';
  import { contactsStore } from './contacts.svelte';
  import { presenceKind, presenceLabel } from './types';
  import { ui } from './ui.svelte';
</script>

<aside class="active" aria-label="Active now">
  <h2 class="title">Active now</h2>
  {#if contactsStore.online.length}
    <ul>
      {#each contactsStore.online as c (c.address)}
        <li>
          <button
            class="row"
            aria-haspopup="dialog"
            onclick={(e) => ui.openPopout(c.address, e.currentTarget, c.name, 'left-start')}
            oncontextmenu={(e) => ui.openPersonMenu(e, c.address, c.name)}
          >
            <Avatar
              name={c.name}
              src={c.avatar}
              size={32}
              presence={presenceKind(c.online, c.show)}
              cut="var(--surface-200)"
            />
            <span class="text">
              <span class="name">{c.name}</span>
              <span class="meta">{c.status ?? presenceLabel[presenceKind(c.online, c.show)]}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="empty">Nobody is active right now.</p>
  {/if}
</aside>

<style>
  .active {
    width: 360px;
    flex: none;
    overflow-y: auto;
    padding: var(--space-4);
    background: var(--surface-200);
    border-left: 1px solid var(--line);
  }
  h2 {
    margin: 0 0 var(--space-3);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 52px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    text-align: left;
    transition: background var(--dur-fast);
  }
  .row:hover {
    background: var(--hover);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    line-height: 18px;
  }
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    margin: var(--space-6) 0;
    color: var(--ink-muted);
  }
</style>
