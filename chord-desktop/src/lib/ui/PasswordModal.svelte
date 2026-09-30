<script lang="ts">
  // A room asks for a password. The join waits for the answer, then it runs again.
  import { onMount } from 'svelte';
  import Modal from './Modal.svelte';
  import { localPart } from './adapt';
  import type { PasswordAsk } from './ui.svelte';

  let { ask, onclose }: { ask: PasswordAsk; onclose: () => void } = $props();

  let value = $state('');
  let input = $state<HTMLInputElement>();
  let answered = false;

  onMount(() => setTimeout(() => input?.focus(), 0));

  function finish(password: string | null) {
    if (answered) return;
    answered = true;
    ask.resolve(password);
    onclose();
  }
</script>

<Modal title="Room password" onclose={() => finish(null)}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      if (value) finish(value);
    }}
  >
    <div class="field">
      <label for="room-password">Password for #{localPart(ask.room)}</label>
      <input
        id="room-password"
        class="input"
        type="password"
        autocomplete="off"
        bind:this={input}
        bind:value
      />
      <span class="meta">
        {ask.again ? 'That password did not work. Try again.' : 'This room needs a password to enter.'}
      </span>
    </div>
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => finish(null)}>Cancel</button>
    <button class="btn btn-primary" disabled={!value} onclick={() => finish(value)}>Join</button>
  {/snippet}
</Modal>
