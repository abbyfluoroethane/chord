<script lang="ts">
  // The "Contacts" row at the top of the DM list on Home.
  import Users from 'lucide-svelte/icons/users';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { contactsStore } from './contacts.svelte';

  const selected = $derived(app.showContacts);
  const count = $derived(contactsStore.pendingCount);
</script>

<div class="wrap">
  <button
    class="row"
    class:selected
    aria-current={selected ? 'page' : undefined}
    aria-label={count ? `Contacts, ${count} pending` : 'Contacts'}
    onclick={() => app.openContacts()}
  >
    <span class="ico"><Icon icon={Users} size={20} /></span>
    <span class="name">Contacts</span>
    {#if count > 0}<span class="badge" aria-hidden="true">{count}</span>{/if}
  </button>
</div>

<style>
  .wrap {
    padding: var(--space-2) var(--space-2) 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 42px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    text-align: left;
    font-weight: 500;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .row:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .row.selected {
    background: var(--selected);
    color: var(--ink);
  }
  .ico {
    display: grid;
    width: 32px;
    place-items: center;
  }
  .name {
    flex: 1;
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
