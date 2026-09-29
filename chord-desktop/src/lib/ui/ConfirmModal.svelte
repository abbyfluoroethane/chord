<script lang="ts">
  // A question before a step that cannot be undone. `ui.confirm` holds the text.
  import Modal from './Modal.svelte';
  import type { ConfirmState } from './ui.svelte';

  let { state: s, onclose }: { state: ConfirmState; onclose: () => void } = $props();
</script>

<Modal title={s.title} {onclose}>
  <p class="ask">{s.text}</p>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button
      class="btn btn-danger"
      onclick={() => {
        onclose();
        s.onconfirm();
      }}>{s.confirm}</button
    >
  {/snippet}
</Modal>

<style>
  .ask {
    margin: 0;
  }
</style>
