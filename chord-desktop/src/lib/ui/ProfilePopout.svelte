<script lang="ts">
  // Profile card, 300px. Opens from member rows, message avatars and names, and contact rows.
  import Popover from './Popover.svelte';
  import ProfileCard from './ProfileCard.svelte';
  import { contactsStore } from './contacts.svelte';
  import { ui, type PopoutState } from './ui.svelte';

  let { state: s }: { state: PopoutState } = $props();

  const p = $derived(contactsStore.person(s.address, s.name));

  function close() {
    ui.popout = null;
  }
</script>

<Popover anchor={s.anchor} onclose={close} placement={s.placement} label="Profile of {p.name}">
  <ProfileCard
    address={s.address}
    name={s.name}
    variant="card"
    cut="var(--surface-300)"
    focusNote={s.focusNote}
    onclose={close}
  />
</Popover>
