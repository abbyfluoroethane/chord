<script lang="ts">
  // Change the password of the account (XEP-0077). The server takes the new password at once.
  // When the keychain holds a password, the app stores the new one there.
  import Modal from './Modal.svelte';
  import { plainError } from './adapt';
  import { api, live } from './bridge';
  import { ui } from './ui.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let next = $state('');
  let again = $state('');
  let busy = $state(false);
  let error = $state('');

  const mismatch = $derived(again !== '' && next !== again);
  const ready = $derived(next !== '' && next === again);

  async function save() {
    if (!ready) return;
    busy = true;
    error = '';
    try {
      if (live) await (await api()).changePassword(next);
      ui.say('Password changed.');
      onclose();
    } catch (e) {
      error = plainError(e);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Change password" {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="field">
      <label for="new-password">New password</label>
      <input
        id="new-password"
        class="input"
        type="password"
        autocomplete="new-password"
        bind:value={next}
        disabled={busy}
      />
    </div>
    <div class="field">
      <label for="repeat-password">Repeat the new password</label>
      <input
        id="repeat-password"
        class="input"
        type="password"
        autocomplete="new-password"
        aria-invalid={mismatch}
        bind:value={again}
        disabled={busy}
      />
      {#if mismatch}<span class="err" role="alert">The two passwords are not the same.</span>{/if}
    </div>
    <span class="meta">Your other devices must sign in again with the new password.</span>
    {#if error}<span class="err" role="alert">{error}</span>{/if}
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={busy || !ready} onclick={() => void save()}>
      Change password
    </button>
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
</style>
