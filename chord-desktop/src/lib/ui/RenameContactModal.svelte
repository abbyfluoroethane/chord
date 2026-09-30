<script lang="ts">
  // Give a contact a new name. The name lives in your contact list (the roster on your
  // server). Only you see it.
  import { onMount } from 'svelte';
  import Modal from './Modal.svelte';
  import { contactsStore } from './contacts.svelte';

  let { target, onclose }: { target: { address: string; name: string }; onclose: () => void } =
    $props();

  let value = $state('');
  let input = $state<HTMLInputElement>();

  onMount(() => {
    value = target.name;
    setTimeout(() => input?.select(), 0);
  });

  function save() {
    // An empty name clears it, and the list shows the address again.
    contactsStore.rename(target.address, value);
    onclose();
  }
</script>

<Modal title="Rename contact" {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    <div class="field">
      <label for="contact-name">Name for {target.address}</label>
      <input id="contact-name" class="input" maxlength="80" bind:this={input} bind:value />
      <span class="meta">Only you see it. Leave it empty to show the address.</span>
    </div>
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" onclick={save}>Save</button>
  {/snippet}
</Modal>
