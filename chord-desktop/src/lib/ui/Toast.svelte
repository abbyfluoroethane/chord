<script lang="ts">
  // A short notice at the bottom of the window. It goes away on its own.
  import { ui } from './ui.svelte';
</script>

<div class="host" role="status" aria-live="polite">
  {#if ui.toast}<div class="toast">{ui.toast}</div>{/if}
</div>

<style>
  .host {
    position: fixed;
    left: 0;
    right: 0;
    bottom: var(--space-6);
    z-index: 90;
    display: flex;
    justify-content: center;
    pointer-events: none;
  }
  .toast {
    padding: var(--space-2) var(--space-4);
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    font-size: 14px;
    font-weight: 500;
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
</style>
