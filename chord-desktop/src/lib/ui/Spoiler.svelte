<script lang="ts">
  // Hidden text. A click, or Enter or Space, shows it. It stays shown for this view.
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  let shown = $state(false);

  function key(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      shown = true;
    }
  }
</script>

{#if shown}<span class="spoiler shown">{@render children()}</span
  >{:else}<span
    class="spoiler"
    role="button"
    tabindex="0"
    aria-label="Spoiler, press to show"
    onclick={() => (shown = true)}
    onkeydown={key}>{@render children()}</span
  >{/if}

<style>
  .spoiler {
    padding: 0 2px;
    border-radius: var(--radius-sm);
  }
  .spoiler:not(.shown) {
    background: var(--ink-muted);
    color: transparent;
    cursor: pointer;
    user-select: none;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .spoiler:not(.shown):hover {
    background: color-mix(in srgb, var(--ink-muted) 80%, var(--ink));
  }
  .spoiler:not(.shown):focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  /* Hidden content stays hidden and does not take clicks, so a link inside does not open. */
  .spoiler:not(.shown) :global(*) {
    color: transparent;
    background: transparent;
    border-color: transparent;
    pointer-events: none;
  }
  .shown {
    background: var(--surface-300);
  }
</style>
