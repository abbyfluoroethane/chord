<script lang="ts">
  // A join would make a new room: the server has none at the address. The join waits for
  // the answer. A typo in the address is the usual reason, so the default is to cancel.
  import Modal from './Modal.svelte';
  import { createRoomQuestion } from './roomjoin';
  import type { CreateRoomAsk } from './ui.svelte';

  let { ask, onclose }: { ask: CreateRoomAsk; onclose: () => void } = $props();

  let answered = false;

  function finish(create: boolean) {
    if (answered) return;
    answered = true;
    ask.resolve(create);
    onclose();
  }
</script>

<Modal title="This room does not exist" onclose={() => finish(false)}>
  <p class="ask">{createRoomQuestion(ask.room)}</p>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={() => finish(false)}>Cancel</button>
    <button class="btn btn-primary" onclick={() => finish(true)}>Make the room</button>
  {/snippet}
</Modal>

<style>
  .ask {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
