<script lang="ts">
  // The search box of the chat header: a field, and a list of hits below it.
  // The search reads the local store, for the open chat only. A click jumps to the message.
  import Search from 'lucide-svelte/icons/search';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { stamp } from './format';
  import { excerpt, jumpToMessage, SEARCH_LIMIT, searchItems, senderLabel } from './search';
  import { ui } from './ui.svelte';
  import type { SearchHit } from '$lib/chord/types';

  let query = $state('');
  let hits = $state<SearchHit[]>([]);
  let open = $state(false);
  let searched = $state('');
  let timer: ReturnType<typeof setTimeout> | undefined;
  let root = $state<HTMLElement>();

  const trimmed = $derived(query.trim());

  async function run(text: string, peer: string) {
    try {
      const found = live
        ? await (await api()).searchMessages(text, peer, SEARCH_LIMIT)
        : searchItems(app.items, peer, text);
      // A newer query, or another chat, has replaced this one.
      if (text === query.trim() && peer === app.selectedJid) {
        hits = found;
        searched = text;
      }
    } catch (e) {
      ui.say(e instanceof Error ? e.message : 'Search failed.');
    }
  }

  function oninput() {
    clearTimeout(timer);
    if (!trimmed || !app.selectedJid) {
      hits = [];
      searched = '';
      return;
    }
    open = true;
    const text = trimmed;
    const peer = app.selectedJid;
    timer = setTimeout(() => void run(text, peer), 150);
  }

  function pick(hit: SearchHit) {
    if (!jumpToMessage(hit.id)) {
      ui.say('That message is older than the loaded ones. Scroll up to load more.');
      return;
    }
    open = false;
  }

  function close() {
    clearTimeout(timer);
    open = false;
    query = '';
    hits = [];
    searched = '';
  }

  // A new chat has new messages: clear the old search.
  $effect(() => {
    void app.selectedJid;
    close();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      close();
      (e.target as HTMLElement).blur();
    }
  }

  function onwindowclick(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onwindowclick} />

<div class="wrap" bind:this={root}>
  <label class="search">
    <Icon icon={Search} size={16} />
    <input
      type="search"
      placeholder="Search"
      aria-label="Search messages in this chat"
      bind:value={query}
      {oninput}
      {onkeydown}
      onfocus={() => (open = trimmed !== '')}
    />
  </label>
  {#if open && trimmed}
    <div class="panel" role="listbox" aria-label="Search results">
      {#if searched !== trimmed}
        <p class="note">Searching…</p>
      {:else if hits.length === 0}
        <p class="note">No messages match “{trimmed}”.</p>
      {:else}
        {#each hits as hit (hit.id)}
          {@const e = excerpt(hit.body, searched)}
          <button class="hit" role="option" aria-selected="false" onclick={() => pick(hit)}>
            <span class="meta">
              <span class="who">{senderLabel(hit)}</span>
              <span class="when">{stamp(hit.timestamp)}</span>
            </span>
            <span class="text">{e.before}<mark>{e.match}</mark>{e.after}</span>
          </button>
        {/each}
        {#if hits.length >= SEARCH_LIMIT}
          <p class="note">Showing the newest {SEARCH_LIMIT} matches.</p>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 168px;
    height: 28px;
    padding: 0 var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-300);
    color: var(--ink-muted);
    transition:
      width var(--dur-arrive) var(--ease-out),
      border-color var(--dur-fast);
  }
  .search:focus-within {
    width: 240px;
    border-color: var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    background: none;
    border: 0;
    outline: 0;
    font-size: 14px;
  }
  input::placeholder {
    color: var(--ink-muted);
  }
  .panel {
    position: absolute;
    top: calc(100% + var(--space-2));
    right: 0;
    z-index: 20;
    width: 360px;
    max-height: 420px;
    overflow-y: auto;
    padding: var(--space-1);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-100);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.25);
  }
  .note {
    margin: 0;
    padding: var(--space-3);
    color: var(--ink-muted);
    font-size: 14px;
  }
  .hit {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    text-align: left;
  }
  .hit:hover,
  .hit:focus-visible {
    background: var(--hover);
  }
  .meta {
    display: flex;
    gap: var(--space-2);
    font-size: 12px;
    color: var(--ink-muted);
  }
  .who {
    color: var(--ink);
    font-weight: 600;
  }
  .text {
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  mark {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
    color: inherit;
    border-radius: 2px;
  }
</style>
