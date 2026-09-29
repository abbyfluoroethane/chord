<script lang="ts">
  import { live } from './bridge';
  import Modal from './Modal.svelte';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let forget = $state(false);

  // Maps to api.logout(), and api.forgetPassword(account) when the box is checked.
  function logout() {
    ui.logoutOpen = false;
    ui.settingsOpen = false;
    void session.signOut(forget);
  }
</script>

<Modal title="Log out" {onclose}>
  <p>Log out of Chord? You need your password to sign in again.</p>
  {#if live && session.hasSavedPassword}
    <label class="check">
      <input type="checkbox" bind:checked={forget} /> Also forget my saved password
    </label>
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-danger" onclick={logout}>Log out</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
</style>
