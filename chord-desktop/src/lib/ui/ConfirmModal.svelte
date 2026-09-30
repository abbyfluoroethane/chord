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
        // `s` is ui.confirm: onclose() clears it, so take the callback first.
        const go = s.onconfirm;
        onclose();
        go();
      }}>{s.confirm}</button
    >
  {/snippet}
</Modal>

<style>
  .ask {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
