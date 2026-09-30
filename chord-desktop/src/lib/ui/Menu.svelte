<script module lang="ts">
  import type { ComponentType, Snippet } from 'svelte';

  export interface MenuItem {
    label: string;
    /** The icon sits on the right of the row, as in Discord. */
    icon?: ComponentType;
    danger?: boolean;
    disabled?: boolean;
    /** Small text on the right. It says why a disabled row is disabled. */
    hint?: string;
    /** Draw a divider above this item. */
    separator?: boolean;
    /** A check mark shows next to the label. */
    checked?: boolean;
    /** A submenu opens to the right. Then onselect is not used. */
    submenu?: MenuItem[];
    /** The emoji picker opens to the right. Then onselect is not used. */
    picker?: (emoji: string) => void;
    onselect?: (e: MouseEvent | KeyboardEvent) => void;
  }

  /** A row can open a panel to its right. */
  const opens = (item: MenuItem | undefined) => !!item && (!!item.submenu || !!item.picker);
</script>

<script lang="ts">
  import Icon from './Icon.svelte';
  import Check from 'lucide-svelte/icons/check';
  import ChevronRight from 'lucide-svelte/icons/chevron-right';
  import EmojiPicker from './EmojiPicker.svelte';
  import Menu from './Menu.svelte';
  import Popover, { type Placement } from './Popover.svelte';

  let {
    anchor,
    items,
    onclose,
    placement = 'bottom-start',
    label,
    nested = false,
    header,
    done,
    fill = false
  }: {
    anchor: HTMLElement;
    items: MenuItem[];
    onclose: () => void;
    placement?: Placement;
    label: string;
    /** True for a submenu. ArrowLeft closes it. */
    nested?: boolean;
    /** A row above the items, for example the quick reactions. Its buttons need role="menuitem". */
    header?: Snippet;
    /** Close the whole menu after a pick. A submenu gets the close of its root. */
    done?: () => void;
    /** As wide as the anchor, less the gap at each side (see Popover). */
    fill?: boolean;
  } = $props();

  let list = $state<HTMLDivElement>();
  let opened = $state<number | null>(null);
  let buttons = $state<HTMLButtonElement[]>([]);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let typed = '';
  let typedTimer: ReturnType<typeof setTimeout> | undefined;

  const rows = () => [
    ...(list?.querySelectorAll<HTMLButtonElement>(':scope > [role="menuitem"]:not(:disabled)') ?? [])
  ];
  const heads = () => [
    ...(list?.querySelectorAll<HTMLButtonElement>(':scope > .head [role="menuitem"]:not(:disabled)') ?? [])
  ];

  function keydown(e: KeyboardEvent) {
    const active = document.activeElement as HTMLButtonElement | null;
    const inNested = !!active && !!list && active.closest('[role="menu"], [role="dialog"]') !== list.closest('[role="menu"]');
    // A submenu sits inside its parent in the DOM. Keep the parent from moving too.
    if (nested && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) e.stopPropagation();
    if (inNested) return;
    const btns = rows();
    const top = heads();
    const inHead = !!active && top.includes(active);
    const i = btns.indexOf(active as HTMLButtonElement);
    if (e.key === 'ArrowRight' && active instanceof HTMLButtonElement && !inHead) {
      const at = buttons.indexOf(active);
      if (at >= 0 && opens(items[at])) {
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
    if (inHead && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
      e.preventDefault();
      const j = top.indexOf(active!) + (e.key === 'ArrowRight' ? 1 : -1);
      top[(j + top.length) % top.length]?.focus();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (inHead) btns[0]?.focus();
      else if (top.length && i === btns.length - 1) top[0].focus();
      else btns[(i + 1) % btns.length]?.focus();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (inHead) btns[btns.length - 1]?.focus();
      else if (top.length && i <= 0) top[0].focus();
      else btns[(i - 1 + btns.length) % btns.length]?.focus();
    } else if (e.key === 'Home') {
      e.preventDefault();
      (top[0] ?? btns[0])?.focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      btns[btns.length - 1]?.focus();
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && e.key !== ' ') {
      // Type-ahead: the first row that starts with the typed letters.
      typed += e.key.toLowerCase();
      clearTimeout(typedTimer);
      typedTimer = setTimeout(() => (typed = ''), 700);
      const order = [...btns.slice(i + 1), ...btns.slice(0, i + 1)];
      const hit = order.find((b) => b.textContent?.trim().toLowerCase().startsWith(typed));
      if (hit) {
        e.preventDefault();
        hit.focus();
      }
    }
  }

  function pick(item: MenuItem, i: number, e: MouseEvent) {
    if (opens(item)) {
      opened = opened === i ? null : i;
      return;
    }
    (done ?? onclose)();
    item.onselect?.(e);
  }

  // A pointer that rests on a row that opens a panel opens it. A pointer that rests on
  // another row closes the panel. Moving across the rows to reach the panel does not.
  function hover(i: number) {
    clearTimeout(hoverTimer);
    if (opens(items[i])) {
      if (opened !== i) hoverTimer = setTimeout(() => (opened = i), 150);
    } else if (opened !== null) {
      hoverTimer = setTimeout(() => (opened = null), 260);
    }
  }

  $effect(() => () => {
    clearTimeout(hoverTimer);
    clearTimeout(typedTimer);
  });
</script>

<Popover {anchor} {onclose} {placement} {label} {fill} role="menu">
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div class="menu" bind:this={list} onkeydown={keydown} role="presentation">
    {#if header}<div class="head" role="group" aria-label="Quick reactions">{@render header()}</div>{/if}
    {#each items as item, i (item.label)}
      {#if item.separator}<div class="sep" role="separator"></div>{/if}
      <button
        bind:this={buttons[i]}
        class="item"
        class:danger={item.danger}
        role="menuitem"
        aria-haspopup={opens(item) ? 'menu' : undefined}
        aria-expanded={opens(item) ? opened === i : undefined}
        disabled={item.disabled}
        title={item.disabled ? item.hint : undefined}
        onclick={(e) => pick(item, i, e)}
        onpointerenter={() => hover(i)}
      >
        <span class="label">{item.label}</span>
        {#if item.disabled && item.hint}<span class="hint">{item.hint}</span>{/if}
        {#if item.checked}<span class="check"><Icon icon={Check} size={16} /></span>{/if}
        {#if item.icon}<span class="glyph"><Icon icon={item.icon} size={16} /></span>{/if}
        {#if opens(item)}<span class="glyph"><Icon icon={ChevronRight} size={16} /></span>{/if}
      </button>
      {#if opens(item) && opened === i && buttons[i]}
        <!-- The wrapper has no box. It tells the menu that the pointer is on the panel. -->
        <div class="panel" role="presentation" onpointerenter={() => clearTimeout(hoverTimer)}>
          {#if item.picker}
            {@const choose = item.picker}
            <EmojiPicker
              anchor={buttons[i]}
              placement="right-start"
              onpick={(emoji) => {
                (done ?? onclose)();
                choose(emoji);
              }}
              onclose={() => {
                opened = null;
                buttons[i]?.focus();
              }}
            />
          {:else if item.submenu}
            <Menu
              nested
              anchor={buttons[i]}
              items={item.submenu}
              placement="right-start"
              label={item.label}
              done={done ?? onclose}
              onclose={() => {
                opened = null;
                buttons[i]?.focus();
              }}
            />
          {/if}
        </div>
      {/if}
    {/each}
  </div>
</Popover>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    padding: var(--space-1);
    min-width: 220px;
  }
  .head {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: var(--space-1);
    padding-bottom: var(--space-1);
  }
  .panel {
    display: contents;
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
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .label {
    flex: 1;
    white-space: nowrap;
  }
  .glyph,
  .check {
    display: grid;
    place-items: center;
    color: var(--ink-muted);
    transition: color var(--dur-fast);
  }
  .check {
    color: var(--brand-ink);
  }
  .hint {
    color: var(--ink-muted);
    font-size: 12px;
  }
  .item:hover:not(:disabled),
  .item:focus-visible,
  .item[aria-expanded='true'] {
    background: var(--hover);
  }
  .item:hover:not(:disabled) .glyph,
  .item:focus-visible .glyph {
    color: var(--ink);
  }
  .item:focus-visible {
    outline-offset: -2px;
  }
  .item:disabled {
    opacity: 0.5;
  }
  .item.danger,
  .item.danger .glyph {
    color: var(--danger);
  }
  .item.danger:hover:not(:disabled),
  .item.danger:focus-visible {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .sep {
    height: 1px;
    margin: var(--space-1) 0;
    background: var(--line);
  }
</style>
