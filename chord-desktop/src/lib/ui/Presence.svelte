<script lang="ts">
  // Presence always pairs colour with a shape:
  // online = filled dot, away = half-filled amber dot, dnd = bar, offline = ring.
  import type { PresenceKind } from './types';
  import { presenceLabel } from './types';

  let { kind, size = 10, label = true }: { kind: PresenceKind; size?: number; label?: boolean } =
    $props();
</script>

<svg
  class="presence"
  width={size}
  height={size}
  viewBox="0 0 10 10"
  role={label ? 'img' : undefined}
  aria-label={label ? presenceLabel[kind] : undefined}
  aria-hidden={label ? undefined : 'true'}
>
  {#if kind === 'online'}
    <circle cx="5" cy="5" r="4.5" fill="var(--online)" />
  {:else if kind === 'away'}
    <circle cx="5" cy="5" r="4" fill="none" stroke="var(--brand)" stroke-width="1.5" />
    <path d="M5 1a4 4 0 0 0 0 8z" fill="var(--brand)" />
  {:else if kind === 'dnd'}
    <circle cx="5" cy="5" r="4.5" fill="var(--danger)" />
    <rect x="2.5" y="4.1" width="5" height="1.8" rx="0.9" fill="var(--cut, var(--surface-200))" />
  {:else}
    <circle cx="5" cy="5" r="3.6" fill="none" stroke="var(--ink-muted)" stroke-width="1.6" />
  {/if}
</svg>

<style>
  .presence {
    display: block;
    flex: none;
  }
</style>
