<script lang="ts">
  // Small grid of common emoji. No dependency.
  import Popover, { type Placement } from './Popover.svelte';
  import { EMOJI } from './emoji';

  let {
    anchor,
    onpick,
    onclose,
    placement = 'bottom-end'
  }: {
    anchor: HTMLElement;
    onpick: (emoji: string) => void;
    onclose: () => void;
    placement?: Placement;
  } = $props();

  let grid = $state<HTMLDivElement>();

  function keydown(e: KeyboardEvent) {
    const btns = [...(grid?.querySelectorAll<HTMLButtonElement>('button') ?? [])];
    const i = btns.indexOf(document.activeElement as HTMLButtonElement);
    const move: Record<string, number> = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: 8, ArrowUp: -8 };
    if (e.key in move) {
      e.preventDefault();
      btns[Math.max(0, Math.min(btns.length - 1, i + move[e.key]))]?.focus();
    }
  }
</script>

<Popover {anchor} {onclose} {placement} label="Pick a reaction">
  <div class="grid" bind:this={grid} onkeydown={keydown} role="presentation">
    {#each EMOJI as e (e)}
      <button
        aria-label="React with {e}"
        onclick={() => {
          onpick(e);
          onclose();
        }}>{e}</button
      >
    {/each}
  </div>
</Popover>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(8, 32px);
    gap: 2px;
    padding: var(--space-2);
  }
  button {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm);
    font-size: 18px;
    line-height: 1;
    transition: background var(--dur-fast);
  }
  button:hover,
  button:focus-visible {
    background: var(--hover);
  }
</style>
