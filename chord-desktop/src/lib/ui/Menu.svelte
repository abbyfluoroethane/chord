<script module lang="ts">
  import type { ComponentType } from 'svelte';

  export interface MenuItem {
    label: string;
    icon?: ComponentType;
    danger?: boolean;
    disabled?: boolean;
    /** Draw a divider above this item. */
    separator?: boolean;
    /** A check mark shows next to the label. */
    checked?: boolean;
    onselect: (e: MouseEvent | KeyboardEvent) => void;
  }
</script>

<script lang="ts">
  import Icon from './Icon.svelte';
  import Check from 'lucide-svelte/icons/check';
  import Popover, { type Placement } from './Popover.svelte';

  let {
    anchor,
    items,
    onclose,
    placement = 'bottom-start',
    label
  }: {
    anchor: HTMLElement;
    items: MenuItem[];
    onclose: () => void;
    placement?: Placement;
    label: string;
  } = $props();

  let list = $state<HTMLDivElement>();

  function keydown(e: KeyboardEvent) {
    const btns = [...(list?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
    const i = btns.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      btns[(i + 1) % btns.length]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      btns[(i - 1 + btns.length) % btns.length]?.focus();
    } else if (e.key === 'Home') {
      e.preventDefault();
      btns[0]?.focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      btns[btns.length - 1]?.focus();
    }
  }

  function pick(item: MenuItem, e: MouseEvent) {
    onclose();
    item.onselect(e);
  }
</script>

<Popover {anchor} {onclose} {placement} {label} role="menu">
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="menu" bind:this={list} onkeydown={keydown} role="presentation">
    {#each items as item (item.label)}
      {#if item.separator}<div class="sep" role="separator"></div>{/if}
      <button
        class="item"
        class:danger={item.danger}
        role="menuitem"
        disabled={item.disabled}
        onclick={(e) => pick(item, e)}
      >
        {#if item.icon}<Icon icon={item.icon} size={16} />{/if}
        <span class="label">{item.label}</span>
        {#if item.checked}<Icon icon={Check} size={16} />{/if}
      </button>
    {/each}
  </div>
</Popover>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    padding: var(--space-1);
    min-width: 200px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    text-align: left;
    font-size: 14px;
    transition: background var(--dur-fast);
  }
  .label {
    flex: 1;
  }
  .item:hover:not(:disabled),
  .item:focus-visible {
    background: var(--hover);
  }
  .item:focus-visible {
    outline-offset: -2px;
  }
  .item:disabled {
    opacity: 0.5;
  }
  .item.danger {
    color: var(--danger);
  }
  .sep {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--line);
  }
</style>
