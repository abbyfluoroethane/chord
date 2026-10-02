<script lang="ts">
  // Full profile, 440px. The same parts as the card, with the shared spaces and contacts below.
  import Modal from './Modal.svelte';
  import ProfileCard from './ProfileCard.svelte';
  import { contactsStore } from './contacts.svelte';

  let { address, onclose }: { address: string; onclose: () => void } = $props();

  const p = $derived(contactsStore.person(address));
</script>

<Modal title="Profile of {p.name}" bare {onclose}>
  <ProfileCard {address} variant="profile" cut="var(--surface-200)" {onclose} />
</Modal>
