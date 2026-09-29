<script lang="ts">
  // Round 36px action button for rows. The label is the tooltip and the aria-label.
  import type { ComponentType } from 'svelte';
  import Icon from './Icon.svelte';
  import { tooltip } from './tooltip';

  let {
    icon,
    label,
    onclick,
    tone = 'normal',
    expanded
  }: {
    icon: ComponentType;
    label: string;
    onclick: (e: MouseEvent) => void;
    tone?: 'normal' | 'good' | 'danger';
    expanded?: boolean;
  } = $props();
</script>

<button
  class="round {tone}"
  aria-label={label}
  aria-haspopup={expanded === undefined ? undefined : 'menu'}
  aria-expanded={expanded}
  use:tooltip={{ text: label, side: 'top' }}
  {onclick}
>
  <Icon {icon} size={18} />
</button>

<style>
  .round {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    flex: none;
    border-radius: 50%;
    background: var(--surface-300);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .round:hover {
    color: var(--ink);
    background: var(--surface-100);
  }
  .round.good:hover {
    color: var(--online);
  }
  .round.danger:hover {
    color: var(--danger);
  }
</style>
