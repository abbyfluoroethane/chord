<script lang="ts">
  // The circle rail: Home, circles, local folders, add button.
  // Drag a circle onto another to make a folder. Drop near an edge to reorder.
  import House from 'lucide-svelte/icons/house';
  import Plus from 'lucide-svelte/icons/plus';
  import CircleIcon from './CircleIcon.svelte';
  import Icon from './Icon.svelte';
  import RailFolder from './RailFolder.svelte';
  import RailItem from './RailItem.svelte';
  import { app, HOME } from './app.svelte';
  import { rail, type DropZone } from './rail.svelte';
  import { spaceKey } from './types';
  import { ui } from './ui.svelte';

  let dragId = $state<string | null>(null);
  let over = $state<{ id: string; zone: DropZone } | null>(null);

  const home = $derived(app.spaceBadge(HOME));

  function circle(id: string) {
    const s = app.spaceOf(id);
    return { id, name: s?.name ?? id, avatar: s?.avatar ?? null };
  }

  function folderBadge(ids: string[]) {
    let unread = 0;
    let mentions = 0;
    for (const id of ids) {
      const b = app.spaceBadge(id);
      unread += b.unread;
      mentions += b.mentions;
    }
    return { unread, mentions };
  }

  function zoneFor(e: DragEvent, canMerge: boolean): DropZone {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const y = (e.clientY - r.top) / r.height;
    if (canMerge && y >= 0.25 && y <= 0.75) return 'into';
    return y < 0.5 ? 'before' : 'after';
  }

  /** Handlers for one draggable slot. */
  function dnd(id: string, canMerge: boolean) {
    return {
      draggable: true,
      ondragstart(e: DragEvent) {
        dragId = id;
        e.dataTransfer?.setData('text/plain', id);
        if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
      },
      ondragover(e: DragEvent) {
        if (!dragId || dragId === id) return;
        e.preventDefault();
        e.stopPropagation();
        const dragIsFolder = rail.layout.some((x) => x.kind === 'folder' && x.id === dragId);
        over = { id, zone: zoneFor(e, canMerge && !dragIsFolder) };
      },
      ondrop(e: DragEvent) {
        if (!dragId || !over) return;
        e.preventDefault();
        e.stopPropagation();
        rail.drop(dragId, over.id, over.zone);
        end();
      },
      ondragend: end
    };
  }

  function end() {
    dragId = null;
    over = null;
  }

  function dropOnRail(e: DragEvent) {
    if (!dragId) return;
    e.preventDefault();
    const last = rail.layout[rail.layout.length - 1];
    if (rail.folderOf(dragId)) rail.release(dragId);
    else if (last) rail.drop(dragId, last.id, 'after');
    end();
  }

  function cls(id: string): string {
    if (!over || over.id !== id) return '';
    return `drop drop-${over.zone}`;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<nav class="rail" aria-label="Circles" ondragover={(e) => dragId && e.preventDefault()} ondrop={dropOnRail}>
  <RailItem
    name="Home and direct messages"
    variant="home"
    selected={app.selectedSpace === HOME}
    unread={home.unread}
    mentions={home.mentions}
    onclick={() => app.selectSpace(HOME)}
  >
    <Icon icon={House} size={22} />
  </RailItem>

  <div class="sep" role="separator"></div>

  {#each rail.layout as entry (entry.id)}
    {#if entry.kind === 'circle'}
      {@const s = circle(entry.id)}
      {@const b = app.spaceBadge(entry.id)}
      <RailItem
        name={s.name}
        selected={app.selectedSpace === entry.id}
        unread={b.unread}
        mentions={b.mentions}
        onclick={() => app.selectSpace(entry.id)}
        class={cls(entry.id)}
        {...dnd(entry.id, true)}
      >
        <CircleIcon name={s.name} src={s.avatar} />
      </RailItem>
    {:else}
      {@const inside = entry.circles.map(circle)}
      {@const fb = folderBadge(entry.circles)}
      <RailFolder
        folder={entry}
        circles={inside}
        selected={entry.circles.includes(app.selectedSpace)}
        unread={fb.unread}
        mentions={fb.mentions}
        ontoggle={() => rail.toggle(entry.id)}
        class={cls(entry.id)}
        {...dnd(entry.id, true)}
      >
        {#each inside as s (s.id)}
          {@const b = app.spaceBadge(s.id)}
          <RailItem
            name={s.name}
            selected={app.selectedSpace === s.id}
            unread={b.unread}
            mentions={b.mentions}
            onclick={() => app.selectSpace(s.id)}
            class={cls(s.id)}
            {...dnd(s.id, false)}
          >
            <CircleIcon name={s.name} src={s.avatar} />
          </RailItem>
        {/each}
      </RailFolder>
    {/if}
  {/each}

  <RailItem name="Add a circle" variant="add" onclick={() => (ui.addCircleOpen = true)}>
    <Icon icon={Plus} size={20} />
  </RailItem>
</nav>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: var(--rail-width);
    padding: var(--space-2) 0;
    background: var(--surface-300);
    border-right: 1px solid var(--line);
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: none;
  }
  .rail::-webkit-scrollbar {
    display: none;
  }
  .sep {
    width: 32px;
    height: 2px;
    margin: var(--space-1) 0;
    border-radius: 1px;
    background: var(--line);
    flex: none;
  }
  :global(.slot.drop-before)::before,
  :global(.slot.drop-after)::after {
    content: '';
    position: absolute;
    left: 12px;
    right: 12px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
  }
  :global(.slot.drop-before)::before {
    top: 0;
  }
  :global(.slot.drop-after)::after {
    bottom: 0;
  }
  :global(.slot.drop-into .tile) {
    border-color: var(--accent);
    border-radius: 16px;
  }
</style>
