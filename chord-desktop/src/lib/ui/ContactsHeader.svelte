<script lang="ts">
  // 48px header of the contacts page: title, tabs, and the amber "Add contact" tab.
  import Users from 'lucide-svelte/icons/users';
  import Icon from './Icon.svelte';
  import { contactsStore } from './contacts.svelte';
  import type { ContactsTab } from './types';

  const tabs: { id: ContactsTab; label: string }[] = [
    { id: 'online', label: 'Online' },
    { id: 'all', label: 'All' },
    { id: 'pending', label: 'Pending' },
    { id: 'blocked', label: 'Blocked' }
  ];

  function key(e: KeyboardEvent) {
    const dir = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const all: ContactsTab[] = [...tabs.map((t) => t.id), 'add'];
    const i = all.indexOf(contactsStore.tab);
    contactsStore.tab = all[(i + dir + all.length) % all.length];
    queueMicrotask(() => document.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus());
  }
</script>

<header class="bar">
  <span class="ico"><Icon icon={Users} size={20} /></span>
  <h1 class="title">Contacts</h1>
  <span class="divider" aria-hidden="true"></span>
  <div class="tabs" role="tablist" aria-label="Contacts" tabindex="-1" onkeydown={key}>
    {#each tabs as t (t.id)}
      <button
        role="tab"
        aria-selected={contactsStore.tab === t.id}
        tabindex={contactsStore.tab === t.id ? 0 : -1}
        class="tab"
        class:on={contactsStore.tab === t.id}
        onclick={() => (contactsStore.tab = t.id)}
      >
        {t.label}
        {#if t.id === 'pending' && contactsStore.pendingCount > 0}
          <span class="badge" aria-label="{contactsStore.pendingCount} incoming">{contactsStore.pendingCount}</span>
        {/if}
      </button>
    {/each}
    <button
      role="tab"
      aria-selected={contactsStore.tab === 'add'}
      tabindex={contactsStore.tab === 'add' ? 0 : -1}
      class="tab add"
      class:on={contactsStore.tab === 'add'}
      onclick={() => (contactsStore.tab = 'add')}>Add contact</button
    >
  </div>
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
  }
  .divider {
    width: 1px;
    height: 24px;
    margin: 0 var(--space-2);
    background: var(--line);
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .tabs:focus {
    outline: none;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: 28px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    font-weight: 500;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .tab:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .tab.on {
    background: var(--selected);
    color: var(--ink);
  }
  .badge {
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--danger);
    color: var(--on-danger);
    font-size: 11px;
    font-weight: 600;
    line-height: 16px;
    text-align: center;
  }
  .tab.add {
    padding: 0 var(--space-3);
    background: var(--brand);
    color: var(--on-brand);
  }
  .tab.add:hover {
    background: color-mix(in srgb, var(--brand) 88%, var(--ink));
  }
  .tab.add.on {
    background: transparent;
    color: var(--brand-ink);
    box-shadow: inset 0 0 0 1px var(--brand);
  }
</style>
