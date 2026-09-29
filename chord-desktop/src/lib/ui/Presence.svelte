<script lang="ts">
  // Presence pairs colour with a shape, as on the website (style guide, .pres):
  // online = filled dot, away = half-filled amber dot with an amber ring,
  // dnd = a red bar, offline = a grey ring.
  // With `cut`, a backdrop of that colour follows the shape, `pad` px wide. It cuts the
  // shape out of an avatar.
  import type { PresenceKind } from './types';
  import { presenceLabel } from './types';

  let {
    kind,
    size = 10,
    label = true,
    cut = null,
    pad = 0
  }: {
    kind: PresenceKind;
    size?: number;
    label?: boolean;
    cut?: string | null;
    pad?: number;
  } = $props();

  // The pad in viewBox units: the shape is 10 units wide.
  const p = $derived(cut ? (pad * 10) / size : 0);
  const box = $derived(size + (cut ? 2 * pad : 0));
</script>

<svg
  class="presence"
  width={box}
  height={box}
  viewBox="{-p} {-p} {10 + 2 * p} {10 + 2 * p}"
  role={label ? 'img' : undefined}
  aria-label={label ? presenceLabel[kind] : undefined}
  aria-hidden={label ? undefined : 'true'}
>
  {#if cut}
    {#if kind === 'dnd'}
      <rect x={-p} y={3 - p} width={10 + 2 * p} height={4 + 2 * p} rx={2 + p} fill={cut} />
    {:else}
      <circle cx="5" cy="5" r={5 + p} fill={cut} />
    {/if}
  {/if}
  {#if kind === 'online'}
    <circle cx="5" cy="5" r="5" fill="var(--online)" />
  {:else if kind === 'away'}
    <path d="M5 0a5 5 0 0 0 0 10z" fill="var(--brand)" />
    <circle cx="5" cy="5" r="4" fill="none" stroke="var(--brand)" stroke-width="2" />
  {:else if kind === 'dnd'}
    <rect x="0" y="3" width="10" height="4" rx="2" fill="var(--danger)" />
  {:else}
    <circle cx="5" cy="5" r="4" fill="none" stroke="var(--ink-muted)" stroke-width="2" />
  {/if}
</svg>

<style>
  .presence {
    display: block;
    flex: none;
  }
</style>
