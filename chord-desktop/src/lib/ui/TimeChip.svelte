<script lang="ts">
  // A Discord timestamp as a small chip. The hover title holds the full date and time.
  import { tick } from './now.svelte';
  import { hourCycleOf } from './prefsdata';
  import { prefs } from './prefs.svelte';
  import { dateOf, formatTimestamp, fullTimestamp, type TimeStyle } from './timestamp';

  let { seconds, style }: { seconds: number; style: TimeStyle } = $props();

  const date = $derived(dateOf(seconds));
  const hourCycle = $derived(hourCycleOf(prefs.timeFormat));
  const label = $derived(
    date
      ? formatTimestamp(date, style, style === 'R' ? tick() : 0, { hourCycle })
      : String(seconds)
  );
  const title = $derived(date ? fullTimestamp(date, { hourCycle }) : undefined);
</script>

{#if date}<time class="chip" datetime={date.toISOString()} {title}>{label}</time>{/if}

<style>
  .chip {
    padding: 0 4px;
    background: var(--surface-300);
    border-radius: var(--radius-sm);
  }
</style>
