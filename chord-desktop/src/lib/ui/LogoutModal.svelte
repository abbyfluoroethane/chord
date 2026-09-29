<script lang="ts">
  import Modal from './Modal.svelte';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';

  let { onclose }: { onclose: () => void } = $props();

  function logout() {
    ui.logoutOpen = false;
    ui.settingsOpen = false;
    session.signOut();
  }
</script>

<Modal title="Log out" {onclose}>
  <p>Log out of Chord? You need your password to sign in again.</p>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-danger" onclick={logout}>Log out</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0;
  }
</style>
