<script lang="ts">
  // The splash screen. It shows from the first frame until the saved session is up.
  // The inline splash in app.html looks the same, so the hand-over is not visible.
  import { onMount } from 'svelte';
  import ChordMark from './ChordMark.svelte';

  let {
    status,
    onCancel,
    slowAfter = 10000
  }: {
    /** The status line under the mark. */
    status: string;
    /** When set, a button shows after `slowAfter` ms. It leaves the wait. */
    onCancel?: () => void;
    slowAfter?: number;
  } = $props();

  let slow = $state(false);

  onMount(() => {
    const timer = setTimeout(() => (slow = true), slowAfter);
    return () => clearTimeout(timer);
  });
</script>

<main class="splash" aria-busy="true">
  <div class="mark"><ChordMark size={72} /></div>
  <p class="meta status" role="status">{status}</p>
  <div class="slot">
    {#if slow && onCancel}
      <button type="button" class="again" onclick={onCancel}>Sign in again</button>
    {/if}
  </div>
</main>

<style>
  .splash {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-4);
    background: var(--surface-100);
    color: var(--ink);
  }
  .mark {
    display: flex;
    animation: pulse 2400ms var(--ease-out) infinite;
  }
  .status {
    margin: 0;
    animation: arrive var(--dur-arrive) var(--ease-out) both;
  }
  .slot {
    height: 32px;
  }
  .again {
    height: 32px;
    padding: 0 var(--space-4);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
    animation: arrive var(--dur-arrive) var(--ease-out) both;
  }
  .again:hover {
    background: var(--hover);
  }
  .again:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  /* The pulse starts at full opacity, so it matches the still mark of app.html. */
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.55;
    }
  }
  @keyframes arrive {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .mark,
    .status,
    .again {
      animation: none;
    }
  }
</style>
