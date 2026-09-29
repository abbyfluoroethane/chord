<script lang="ts">
  // The contacts page. It replaces the chat area on Home. Active now sits in the right column.
  import ActiveNow from './ActiveNow.svelte';
  import AddContact from './AddContact.svelte';
  import ContactsHeader from './ContactsHeader.svelte';
  import ContactsList from './ContactsList.svelte';
  import { contactsStore } from './contacts.svelte';
</script>

<section class="page" aria-label="Contacts">
  <ContactsHeader />
  <div class="body">
    {#if contactsStore.tab === 'add'}
      <div class="main"><AddContact /></div>
    {:else}
      <ContactsList />
    {/if}
    <ActiveNow />
  </div>
</section>

<style>
  .page {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    background: var(--surface-100);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .main {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }
</style>
