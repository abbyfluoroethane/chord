<script module lang="ts">
  export type Placement = 'bottom-start' | 'bottom-end' | 'top-end' | 'left-start' | 'right-start';
</script>

<script lang="ts">
  import { untrack } from 'svelte';
  // Floating panel anchored to an element. Fixed position so scroll
  // containers never clip it. Esc and an outside click close it.
  import type { Snippet } from 'svelte';
  import { ui } from './ui.svelte';

  let {
    anchor,
    onclose,
    placement = 'bottom-start',
    label,
    role = 'dialog',
    fill = false,
    children
  }: {
    anchor: HTMLElement;
    onclose: () => void;
    placement?: Placement;
    label: string;
    role?: 'dialog' | 'menu';
    /** As wide as the anchor, with the same gap at the sides as between the anchor and the panel. */
    fill?: boolean;
    children: Snippet;
  } = $props();

  let el = $state<HTMLDivElement>();

  function place() {
    if (!el) return;
    const a = anchor.getBoundingClientRect();
    const gap = 4;
    if (fill) el.style.width = `${a.width - 2 * gap}px`;
    const p = el.getBoundingClientRect();
    let x = fill ? a.left + gap : a.left;
    let y = a.bottom + gap;
    if (placement === 'bottom-end' && !fill) x = a.right - p.width;
    if (placement === 'top-end') {
      if (!fill) x = a.right - p.width;
      y = a.top - p.height - gap;
    }
    if (placement === 'left-start') {
      x = a.left - p.width - gap;
      y = a.top;
    }
    if (placement === 'right-start') {
      x = a.right + gap;
      y = a.top;
    }
    // Flip vertically when the panel would run off the window.
    if (y + p.height > window.innerHeight - 8 && a.top - p.height - gap > 8 && placement.startsWith('bottom'))
      y = a.top - p.height - gap;
    x = Math.max(8, Math.min(x, window.innerWidth - p.width - 8));
    y = Math.max(8, Math.min(y, window.innerHeight - p.height - 8));
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    el.style.visibility = 'visible';
  }

  $effect(() => {
    if (!el) return;
    place();
    untrack(() => (ui.overlays += 1));
    const open = document.activeElement as HTMLElement | null;
    const first = el.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled), input, button:not(:disabled)');
    (first ?? el).focus({ preventScroll: true });

    const down = (e: PointerEvent) => {
      const t = e.target as Node;
      // A menu that a control in the panel opens is part of the panel.
      const inMenu = t instanceof Element && !!t.closest('[role="menu"]');
      if (!el?.contains(t) && !anchor.contains(t) && !inMenu) onclose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onclose();
      }
    };
    const resize = () => place();
    // Place it again when its content changes size, for example a field that grows.
    const observer = new ResizeObserver(() => place());
    observer.observe(el);
    document.addEventListener('pointerdown', down, true);
    window.addEventListener('keydown', key);
    window.addEventListener('resize', resize);
    return () => {
      observer.disconnect();
      document.removeEventListener('pointerdown', down, true);
      window.removeEventListener('keydown', key);
      window.removeEventListener('resize', resize);
      untrack(() => (ui.overlays -= 1));
      // Give focus back when it was inside the panel.
      if (el?.contains(document.activeElement) || document.activeElement === document.body) open?.focus?.();
    };
  });
</script>

<div bind:this={el} class="popover" class:fill {role} aria-label={label} tabindex="-1">
  {@render children()}
</div>

<style>
  .popover {
    position: fixed;
    left: 0;
    top: 0;
    visibility: hidden;
    z-index: 60;
    min-width: 180px;
    max-height: calc(100vh - 16px);
    overflow: auto;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .fill {
    min-width: 0;
  }
  .popover:focus {
    outline: none;
  }
</style>
