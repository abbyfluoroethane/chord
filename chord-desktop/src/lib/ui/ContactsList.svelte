<script lang="ts">
  // The list under the tabs: search box, count line, rows, or one plain empty line.
  import Search from 'lucide-svelte/icons/search';
  import ContactRow from './ContactRow.svelte';
  import Icon from './Icon.svelte';
  import { contactsStore } from './contacts.svelte';
  import type { ContactItem } from './types';

  type Entry = { item: ContactItem; kind: 'contact' | 'incoming' | 'outgoing' | 'blocked' };

  const byName = (a: ContactItem, b: ContactItem) => a.name.localeCompare(b.name);

  const all = $derived.by<Entry[]>(() => {
    const s = contactsStore;
    switch (s.tab) {
      case 'online':
        return [...s.online].sort(byName).map((item) => ({ item, kind: 'contact' as const }));
      case 'all':
        return [...s.contacts].sort(byName).map((item) => ({ item, kind: 'contact' as const }));
      case 'pending':
        return [
          ...[...s.incoming].sort(byName).map((item) => ({ item, kind: 'incoming' as const })),
          ...[...s.outgoing].sort(byName).map((item) => ({ item, kind: 'outgoing' as const }))
        ];
      case 'blocked':
        return [...s.blocked].sort(byName).map((item) => ({ item, kind: 'blocked' as const }));
      default:
        return [];
    }
  });

  const shown = $derived.by(() => {
    const q = contactsStore.query.trim().toLowerCase();
    if (!q) return all;
    return all.filter(
      (e) => e.item.name.toLowerCase().includes(q) || e.item.address.toLowerCase().includes(q)
    );
  });

  const heading = $derived(
    { online: 'Online', all: 'All contacts', pending: 'Pending', blocked: 'Blocked', add: '' }[contactsStore.tab]
  );

  const empty = $derived(
    contactsStore.query.trim() && all.length
      ? 'No one matches that search.'
      : {
          online: 'Nobody is online right now.',
          all: 'You have no contacts yet. Add one with their address.',
          pending: 'You have no requests.',
          blocked: 'You have not blocked anyone.',
          add: ''
        }[contactsStore.tab]
  );
</script>

<div class="list">
  <label class="search">
    <input
      type="search"
      placeholder="Search"
      aria-label="Search contacts"
      bind:value={contactsStore.query}
    />
    <Icon icon={Search} size={16} />
  </label>

  {#if shown.length}
    <h2 class="group" aria-live="polite">{heading} — {shown.length}</h2>
    <ul>
      {#each shown as e (e.kind + e.item.address)}
        <li><ContactRow item={e.item} kind={e.kind} /></li>
      {/each}
    </ul>
  {:else}
    <p class="empty">{empty}</p>
  {/if}
</div>

<style>
  .list {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding-bottom: var(--space-6);
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-4) var(--space-6) 0;
    height: 32px;
    padding: 0 var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-300);
    color: var(--ink-muted);
  }
  .search:focus-within {
    border-color: var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    background: none;
    border: 0;
    outline: 0;
  }
  input::placeholder {
    color: var(--ink-muted);
  }
  .group {
    margin: var(--space-4) var(--space-6) var(--space-2);
    font-size: 12px;
    line-height: 16px;
    font-weight: 500;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .empty {
    margin: var(--space-16) var(--space-6) 0;
    text-align: center;
    color: var(--ink-muted);
  }
</style>
