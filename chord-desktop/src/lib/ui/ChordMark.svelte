<script lang="ts">
  // The Chord mark: a ring with one chord across it, and a dot at each end.
  // Geometry from the style guide (chord-mark.svg, 64 x 64). One colour: currentColor.
  // At 16px and below the style guide drops the dots.
  let { size = 24, label }: { size?: number; label?: string } = $props();
  const dots = $derived(size > 16);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 64 64"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <circle cx="32" cy="32" r="22" fill="none" stroke="currentColor" stroke-width="4" />
  <line x1="11.3" y1="39.5" x2="39.5" y2="11.3" stroke="currentColor" stroke-width="4" stroke-linecap="round" />
  {#if dots}
    <circle cx="11.3" cy="39.5" r="5" fill="currentColor" />
    <circle cx="39.5" cy="11.3" r="5" fill="currentColor" />
  {/if}
</svg>
