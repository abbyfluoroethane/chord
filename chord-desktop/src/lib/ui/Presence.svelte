<script lang="ts">
  // Presence pairs colour with a shape, as on the website (style guide, .pres):
  // online = filled dot, away = half-filled amber dot with an amber ring,
  // dnd = a red bar, offline = a grey ring.
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
