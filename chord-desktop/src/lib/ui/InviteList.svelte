<script lang="ts">
  // Your contacts, with a search and a check box on each row. The invite dialog uses it
  // to pick the people who get an invite.
  import Check from 'lucide-svelte/icons/check';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import { contactsStore } from './contacts.svelte';
  import { score } from './fuzzy';

  let { picked = $bindable([]) }: { picked?: string[] } = $props();

  let query = $state('');

  const shown = $derived(
    contactsStore.contacts
      .map((c) => ({ c, s: Math.max(score(query, c.name), score(query, c.address)) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => (query ? b.s - a.s : a.c.name.localeCompare(b.c.name)))
      .map((x) => x.c)
  );

  function toggle(address: string) {
    picked = picked.includes(address) ? picked.filter((a) => a !== address) : [...picked, address];
  }
</script>

<input
  class="input search"
  type="search"
  placeholder="Search your contacts"
  aria-label="Search your contacts"
  autocomplete="off"
  spellcheck="false"
  bind:value={query}
/>
<ul class="list" role="listbox" aria-multiselectable="true" aria-label="Contacts">
  {#each shown as c (c.address)}
    {@const on = picked.includes(c.address)}
    <li role="option" aria-selected={on}>
      <button type="button" class="row" class:on onclick={() => toggle(c.address)}>
        <Avatar name={c.name} src={c.avatar} size={32} />
        <span class="text">
          <span class="name">{c.name}</span>
          <span class="addr mono">{c.address}</span>
        </span>
        <span class="box" aria-hidden="true">{#if on}<Icon icon={Check} size={14} />{/if}</span>
      </button>
    </li>
  {:else}
    <li class="empty">
      {contactsStore.contacts.length ? 'No contact matches.' : 'You have no contacts yet.'}
    </li>
  {/each}
</ul>

<style>
  .search {
    width: 100%;
    margin-bottom: var(--space-2);
  }
  .list {
    max-height: 264px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-md);
    text-align: left;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .row:hover {
    background: var(--hover);
  }
  .row.on {
    background: var(--selected);
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .addr {
    overflow: hidden;
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A square check box: an empty line box, or amber with a check. */
  .box {
    display: grid;
    flex: none;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    color: var(--on-brand);
    transition:
      background var(--dur-fast) var(--ease-out),
      border-color var(--dur-fast) var(--ease-out);
  }
  .row.on .box {
    background: var(--brand);
    border-color: var(--brand);
  }
  .empty {
    padding: var(--space-4) var(--space-2);
    color: var(--ink-muted);
    text-align: center;
  }
</style>
