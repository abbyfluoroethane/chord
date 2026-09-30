<script lang="ts">
  // One slot on the circle rail: the left pill, a tile, a mention badge.
  // The rail stays 64px wide. A round tile is 48px. A squircle tile is 44px, because a
  // squircle of the same width looks larger than a circle. The two have about the same area.
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import { tooltip } from './tooltip';

  let {
    name,
    selected = false,
    unread = 0,
    mentions = 0,
    variant = 'circle',
    onclick,
    children,
    class: klass = '',
    ...rest
  }: {
    name: string;
    selected?: boolean;
    unread?: number;
    mentions?: number;
    variant?: 'circle' | 'home' | 'add' | 'folder';
    onclick: () => void;
    children: Snippet;
  } & Omit<HTMLAttributes<HTMLDivElement>, 'children' | 'onclick'> = $props();

  const hasUnread = $derived(unread > 0 || mentions > 0);
</script>

<div class="slot {klass}" {...rest}>
  <span class="pill" class:selected class:unread={hasUnread && !selected}></span>
  <button
    class="tile {variant}"
    class:selected
    aria-label={name +
      (mentions ? `, ${mentions} ${mentions === 1 ? 'mention' : 'mentions'}` : '') +
      (!mentions && unread ? ', unread' : '')}
    aria-current={selected ? 'true' : undefined}
    use:tooltip={name}
    {onclick}
  >
    {@render children()}
  </button>
  {#if mentions > 0}
    <span class="badge" aria-hidden="true">{mentions > 99 ? '99+' : mentions}</span>
  {/if}
</div>

<style>
  .slot {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--rail-width);
    height: 56px;
    flex: none;
  }
  .tile {
    position: relative;
    display: grid;
    place-items: center;
    width: 48px;
    height: 48px;
    /* Round at rest. Hover and selection turn the tile into a squircle. */
    border-radius: 50%;
    border: 2px solid transparent;
    /* A space tile shows its own picture, so the tile itself has no fill. A fill would
       show as a ring in the 2px border. */
    background: transparent;
    color: var(--ink);
    font-weight: 600;
    font-size: 16px;
    overflow: hidden;
    transition:
      border-width var(--dur-fast) var(--ease-out),
      width var(--dur-fast) var(--ease-out),
      height var(--dur-fast) var(--ease-out),
      border-radius var(--dur-fast) var(--ease-out),
      border-color var(--dur-fast),
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .tile:hover,
  .tile:focus-visible,
  .tile.selected,
  .tile.folder {
    width: 44px;
    height: 44px;
    /* A squircle has no ring: the face fills the tile. The pill shows the selection. */
    border-width: 0;
    border-radius: var(--radius-circle-icon);
  }
  /* Home, with the Chord mark: a neutral circle at rest, like the circles. Hover and
     selection make it an amber squircle. */
  .tile.home:hover,
  .tile.home:focus-visible,
  .tile.home.selected {
    background: var(--brand);
    color: var(--on-brand);
  }
  .tile.add {
    background: transparent;
    border: 2px dashed var(--ink-muted);
    color: var(--ink-muted);
  }
  .tile.add:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  /* Home and a closed folder sit on a surface one step lighter than the rail. */
  .tile.home,
  .tile.folder {
    background: var(--surface-raised);
  }
  .tile.folder {
    padding: 3px;
    /* A closed folder shows a 2x2 grid, so it stays a squircle. */
    border-radius: var(--radius-circle-icon);
  }

  .pill {
    position: absolute;
    left: 0;
    top: 50%;
    width: 4px;
    height: 0;
    transform: translateY(-50%);
    border-radius: 0 2px 2px 0;
    background: var(--ink);
    transition: height var(--dur-fast) var(--ease-out);
  }
  .pill.unread {
    height: 8px;
  }
  .slot:hover .pill:not(.selected) {
    height: 20px;
  }
  .pill.selected {
    height: 40px;
  }

  .badge {
    position: absolute;
    right: 4px;
    bottom: 0;
    min-width: 20px;
    height: 20px;
    padding: 0 5px;
    border-radius: 10px;
    border: 3px solid var(--surface-300);
    background: var(--brand);
    color: var(--on-brand);
    font-size: 11px;
    font-weight: 600;
    text-align: center;
    pointer-events: none;
    box-sizing: border-box;
    line-height: 14px;
  }
</style>
