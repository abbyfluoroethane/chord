<script lang="ts">
  // Draws the one open context menu (see contextmenu.svelte.ts). It closes on a scroll
  // outside the menu, on a resize, and when the window loses focus. Esc and a click
  // outside close it in Popover.
  import Emoji from './Emoji.svelte';
  import Menu from './Menu.svelte';
  import { contextMenu } from './contextmenu.svelte';

  const s = $derived(contextMenu.state);

  $effect(() => {
    if (!s) return;
    const close = () => contextMenu.close();
    const scroll = (e: Event) => {
      const t = e.target as Element | null;
      // The menu and the emoji picker can scroll on their own.
      if (t && typeof t.closest === 'function' && t.closest('.popover')) return;
      close();
    };
    window.addEventListener('blur', close);
    window.addEventListener('resize', close);
    document.addEventListener('scroll', scroll, true);
    return () => {
      window.removeEventListener('blur', close);
      window.removeEventListener('resize', close);
      document.removeEventListener('scroll', scroll, true);
    };
  });
</script>

{#snippet head()}
  {#each s?.quick?.emojis ?? [] as emoji (emoji)}
    <button
      class="tile"
      role="menuitem"
      aria-label="React with {emoji}"
      onclick={() => {
        const pick = s?.quick?.onpick;
        contextMenu.close();
        pick?.(emoji);
      }}><Emoji {emoji} /></button
    >
  {/each}
{/snippet}

{#if s}
  <Menu
    anchor={s.anchor}
    items={s.items}
    placement={s.placement}
    label={s.label}
    onclose={() => contextMenu.close()}
    header={s.quick ? head : undefined}
  />
{/if}

<style>
  .tile {
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    border-radius: var(--radius-md);
    background: var(--hover);
    font-size: 20px;
    line-height: 1;
    transition:
      background var(--dur-fast),
      box-shadow var(--dur-fast);
  }
  .tile:hover,
  .tile:focus-visible {
    background: var(--brand-soft);
    box-shadow: inset 0 0 0 1px var(--brand);
  }
  .tile:focus-visible {
    outline-offset: -2px;
  }
</style>
