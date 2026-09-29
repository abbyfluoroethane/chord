<script lang="ts">
  // A local folder on the rail. Closed: a 2x2 mini grid. Open: the circles below it.
  import Folder from 'lucide-svelte/icons/folder-open';
  import type { HTMLAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';
  import CircleIcon from './CircleIcon.svelte';
  import Icon from './Icon.svelte';
  import RailItem from './RailItem.svelte';
  import type { RailFolder } from './rail.svelte';
  import type { FolderCircle } from './types';

  let {
    folder,
    circles,
    selected,
    unread,
    mentions,
    ontoggle,
    children,
    ...rest
  }: {
    folder: RailFolder;
    circles: FolderCircle[];
    selected: boolean;
    unread: number;
    mentions: number;
    ontoggle: () => void;
    /** The open folder renders its members here. */
    children: Snippet;
  } & Omit<HTMLAttributes<HTMLDivElement>, 'children'> = $props();

  const label = $derived(folder.name ?? circles.map((c) => c.name).join(', '));
</script>

<div class="folder" class:open={folder.open}>
  <RailItem
    name={label}
    variant="folder"
    selected={selected && !folder.open}
    unread={folder.open ? 0 : unread}
    mentions={folder.open ? 0 : mentions}
    onclick={ontoggle}
    {...(rest as Record<string, any>)}
  >
    {#if folder.open}
      <Icon icon={Folder} size={20} />
    {:else}
      <span class="grid">
        {#each circles.slice(0, 4) as c (c.id)}
          <span class="mini"><CircleIcon name={c.name} src={c.avatar} fill /></span>
        {/each}
      </span>
    {/if}
  </RailItem>
  {#if folder.open}
    <div class="members" role="group" aria-label={label}>{@render children()}</div>
  {/if}
</div>

<style>
  .folder.open {
    background: var(--surface-200);
    border-radius: var(--radius-circle-icon);
    margin: 0 4px;
    width: calc(var(--rail-width) - 8px);
    padding-bottom: 2px;
  }
  .folder.open :global(.slot) {
    width: calc(var(--rail-width) - 8px);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
    gap: 2px;
    width: 100%;
    height: 100%;
  }
  .mini {
    display: block;
    font-size: 7px;
    overflow: hidden;
    border-radius: 4px;
  }
  .mini :global(.face) {
    font-size: 7px;
  }
  .members {
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
</style>
