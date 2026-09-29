<script lang="ts" generics="T extends string">
  // A row of exclusive choices. Arrow keys move the choice, as in a radio group.
  let {
    value,
    options,
    label,
    onchange
  }: {
    value: T;
    options: { value: T; label: string }[];
    label: string;
    onchange: (v: T) => void;
  } = $props();

  let group = $state<HTMLDivElement>();

  function key(e: KeyboardEvent) {
    const dir = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const i = options.findIndex((o) => o.value === value);
    const next = options[(i + dir + options.length) % options.length];
    onchange(next.value);
    queueMicrotask(() => group?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
  }
</script>

<div class="seg" role="radiogroup" aria-label={label} bind:this={group} onkeydown={key} tabindex="-1">
  {#each options as o (o.value)}
    <button
      role="radio"
      aria-checked={o.value === value}
      tabindex={o.value === value ? 0 : -1}
      class:on={o.value === value}
      onclick={() => onchange(o.value)}>{o.label}</button
    >
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .seg:focus {
    outline: none;
  }
  button {
    height: 28px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    font-size: 14px;
    font-weight: 500;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  button:hover {
    color: var(--ink);
  }
  button.on {
    background: var(--selected);
    color: var(--ink);
  }
</style>
