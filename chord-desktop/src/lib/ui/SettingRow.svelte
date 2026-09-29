<script lang="ts">
  // One setting: a title and a short hint on the left, the control on the right.
  import type { Snippet } from 'svelte';

  let {
    title,
    hint = '',
    children
  }: { title: string; hint?: string; children: Snippet } = $props();
</script>

<div class="row">
  <div class="text">
    <span class="title-line">{title}</span>
    {#if hint}<span class="meta">{hint}</span>{/if}
  </div>
  <div class="control">{@render children()}</div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--line);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title-line {
    font-weight: 500;
  }
  .control {
    flex: none;
  }
</style>
