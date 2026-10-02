<script lang="ts">
  // The right rail of a 1:1 DM: the profile of the peer, where a room shows its members.
  // The member list button in the header shows and hides it (ui.membersOpen).
  import ProfileCard from './ProfileCard.svelte';
  import { contactsStore } from './contacts.svelte';

  let { address, name }: { address: string; name: string } = $props();

  const p = $derived(contactsStore.person(address, name));
</script>

<aside class="profile" aria-label="Profile of {p.name}">
  <ProfileCard {address} {name} variant="rail" cut="var(--surface-200)" />
</aside>

<style>
  .profile {
    width: var(--members-width);
    flex: none;
    overflow-y: auto;
    background: var(--surface-200);
    border-left: 1px solid var(--line);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
</style>
