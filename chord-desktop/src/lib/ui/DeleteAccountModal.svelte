<script lang="ts">
  // Delete the account on the server (XEP-0077). It cannot be undone, so the user types the
  // address. The button stays off until the text matches. Core checks the address again.
  import Modal from './Modal.svelte';
  import { plainError } from './adapt';
  import { api, live } from './bridge';
  import { app } from './app.svelte';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let typed = $state('');
  let busy = $state(false);
  let error = $state('');

  const address = $derived(app.me.address);
  const matches = $derived(typed.trim().toLowerCase() === address.toLowerCase() && address !== '');

  // Maps to api.deleteAccount(confirm). The server drops the account, so log out after it.
  async function remove() {
    if (!matches || busy) return;
    busy = true;
    error = '';
    try {
      if (live) await (await api()).deleteAccount(typed.trim());
    } catch (e) {
      error = plainError(e);
      busy = false;
      return;
    }
    ui.settingsOpen = false;
    onclose();
    await session.signOut(true);
    ui.say('Your account was deleted.');
  }
</script>

<Modal title="Delete account" {onclose}>
  <form
    id="delete-account"
    onsubmit={(e) => {
      e.preventDefault();
      void remove();
    }}
  >
    <p class="warn">
      This deletes <strong class="mono">{address}</strong> on your server. It cannot be undone.
    </p>
    <ul>
      <li>Your contacts, your rooms and your messages on the server are lost.</li>
      <li>The address may be free for someone else to register.</li>
      <li>Chord signs you out and forgets your saved password.</li>
    </ul>
    <div class="field">
      <label for="delete-confirm">Type your address to confirm</label>
      <input
        id="delete-confirm"
        class="input mono"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        placeholder={address}
        bind:value={typed}
        disabled={busy}
      />
    </div>
    {#if error}<span class="err" role="alert">{error}</span>{/if}
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-danger" type="submit" form="delete-account" disabled={busy || !matches}>
      {busy ? 'Deleting…' : 'Delete my account'}
    </button>
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .warn {
    margin: 0;
    overflow-wrap: anywhere;
  }
  ul {
    margin: 0;
    padding-left: var(--space-5, 20px);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
</style>
