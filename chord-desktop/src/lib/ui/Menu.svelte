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
    /** A submenu opens to the right. Then onselect is not used. */
    submenu?: MenuItem[];
    onselect: (e: MouseEvent | KeyboardEvent) => void;
  }
</script>

<script lang="ts">
  import Icon from './Icon.svelte';
  import Check from 'lucide-svelte/icons/check';
  import ChevronRight from 'lucide-svelte/icons/chevron-right';
  import Menu from './Menu.svelte';
  import Popover, { type Placement } from './Popover.svelte';

  let {
    anchor,
    items,
    onclose,
    placement = 'bottom-start',
    label,
    nested = false
  }: {
    anchor: HTMLElement;
    items: MenuItem[];
    onclose: () => void;
    placement?: Placement;
    label: string;
    /** True for a submenu. ArrowLeft closes it. */
    nested?: boolean;
  } = $props();

  let list = $state<HTMLDivElement>();
  let opened = $state<number | null>(null);
  let buttons = $state<HTMLButtonElement[]>([]);

  function keydown(e: KeyboardEvent) {
    // A submenu sits inside its parent in the DOM. Keep the parent from moving too.
    if (nested && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) e.stopPropagation();
    const btns = [
      ...(list?.querySelectorAll<HTMLButtonElement>(':scope > [role="menuitem"]:not(:disabled)') ?? [])
    ];
    const i = btns.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'ArrowRight' && document.activeElement instanceof HTMLButtonElement) {
      const at = buttons.indexOf(document.activeElement);
      if (at >= 0 && items[at]?.submenu) {
        e.preventDefault();
        e.stopPropagation();
        opened = at;
        return;
      }
    }
    if (e.key === 'ArrowLeft' && nested) {
      e.preventDefault();
      e.stopPropagation();
      onclose();
      return;
    }
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

  function pick(item: MenuItem, i: number, e: MouseEvent) {
    if (item.submenu) {
      opened = opened === i ? null : i;
      return;
    }
    onclose();
    item.onselect(e);
  }
</script>

<Popover {anchor} {onclose} {placement} {label} role="menu">
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="menu" bind:this={list} onkeydown={keydown} role="presentation">
    {#each items as item, i (item.label)}
      {#if item.separator}<div class="sep" role="separator"></div>{/if}
      <button
        bind:this={buttons[i]}
        class="item"
        class:danger={item.danger}
        role="menuitem"
        aria-haspopup={item.submenu ? 'menu' : undefined}
        aria-expanded={item.submenu ? opened === i : undefined}
        disabled={item.disabled}
        onclick={(e) => pick(item, i, e)}
      >
        {#if item.icon}<Icon icon={item.icon} size={16} />{/if}
        <span class="label">{item.label}</span>
        {#if item.checked}<Icon icon={Check} size={16} />{/if}
        {#if item.submenu}<Icon icon={ChevronRight} size={16} />{/if}
      </button>
      {#if item.submenu && opened === i && buttons[i]}
        <Menu
          nested
          anchor={buttons[i]}
          items={item.submenu}
          placement="right-start"
          label={item.label}
          onclose={() => {
            opened = null;
            buttons[i]?.focus();
          }}
        />
      {/if}
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
