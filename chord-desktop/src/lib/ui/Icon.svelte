<script lang="ts">
  // Thin wrapper: Lucide icons at stroke width 1.5, hidden from screen readers.
  // Buttons that hold an icon carry the aria-label.
  import type { ComponentType } from 'svelte';

  let { icon, size = 18 }: { icon: ComponentType; size?: number } = $props();
  const Comp = $derived(icon);
</script>

<Comp {size} strokeWidth={1.5} aria-hidden="true" />
