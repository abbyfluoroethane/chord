<script lang="ts">
  // The GIF panel: KLIPY search in two columns, trending before a search. A pick sends the
  // GIF as a link with an embed (XEP-0066). KLIPY asks for "Search KLIPY" in the field
  // and "Powered by KLIPY" on the panel.
  import { onMount } from 'svelte';
  import type { Gif } from '$lib/chord';
  import * as fx from '$lib/fixtures/data';
  import { plainError } from './adapt';
  import { api, live } from './bridge';

  let { onpick }: { onpick: (gif: Gif) => void } = $props();

  const DEBOUNCE_MS = 300;

  let query = $state('');
  let items = $state<Gif[]>([]);
  let page = $state(1);
  let hasNext = $state(false);
  let loading = $state(false);
  let error = $state<{ text: string; retry: boolean } | null>(null);
  let search = $state<HTMLInputElement>();
  let scroller = $state<HTMLDivElement>();
  // Only the answer to the newest request counts.
  let request = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function fetchPage(q: string, n: number): Promise<{ items: Gif[]; hasNext: boolean }> {
    if (!live) {
      const t = q.trim().toLowerCase();
      const list = t ? fx.gifs.filter((g) => g.title.toLowerCase().includes(t)) : fx.gifs;
      return { items: n === 1 ? list : [], hasNext: false };
    }
    return (await api()).gifSearch(q, n);
  }

  async function load(reset: boolean) {
    const id = ++request;
    const n = reset ? 1 : page + 1;
    loading = true;
    error = null;
    try {
      const result = await fetchPage(query, n);
      if (id !== request) return;
      items = reset ? result.items : [...items, ...result.items];
      page = n;
      hasNext = result.hasNext;
      if (reset && scroller) scroller.scrollTop = 0;
    } catch (e) {
      if (id !== request) return;
      const code = (e as { code?: string } | null)?.code;
      error =
        code === 'gifUnavailable'
          ? { text: 'GIF search is not set up in this build of Chord.', retry: false }
          : { text: plainError(e), retry: true };
    } finally {
      if (id === request) loading = false;
    }
  }

  function typed() {
    clearTimeout(timer);
    timer = setTimeout(() => void load(true), DEBOUNCE_MS);
  }

  // Load the next page when the end of the list comes near.
  function scrolled() {
    if (!scroller || loading || !hasNext || error) return;
    if (scroller.scrollTop + scroller.clientHeight > scroller.scrollHeight - 300) void load(false);
  }

  onMount(() => {
    void load(true);
    search?.focus();
    return () => clearTimeout(timer);
  });

  // Two columns of about the same height: each GIF goes to the shorter one.
  const columns = $derived.by(() => {
    const cols: Gif[][] = [[], []];
    const heights = [0, 0];
    for (const g of items) {
      const h = g.preview.width > 0 ? g.preview.height / g.preview.width : 1;
      const i = heights[0] <= heights[1] ? 0 : 1;
      cols[i].push(g);
      heights[i] += h;
    }
    return cols;
  });

  function searchKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      scroller?.querySelector<HTMLButtonElement>('.tile')?.focus();
    }
  }
</script>

<div class="panel" role="dialog" aria-label="Pick a GIF">
  <div class="head">
    <input
      bind:this={search}
      bind:value={query}
      class="input search"
      type="search"
      placeholder="Search KLIPY"
      aria-label="Search KLIPY for a GIF"
      autocomplete="off"
      spellcheck="false"
      oninput={typed}
      onkeydown={searchKey}
    />
  </div>

  <div class="scroll" bind:this={scroller} onscroll={scrolled}>
    <h3>{query.trim() ? 'Results' : 'Trending'}</h3>
    {#if error}
      <div class="state">
        <p>{error.text}</p>
        {#if error.retry}
          <button class="btn" onclick={() => load(items.length === 0)}>Try again</button>
        {/if}
      </div>
    {:else if !loading && items.length === 0}
      <div class="state"><p>No GIFs found.</p></div>
    {:else}
      <div class="columns">
        {#each columns as col, c (c)}
          <div class="col">
            {#each col as g (g.slug)}
              <button
                class="tile"
                style:aspect-ratio="{g.preview.width || 1} / {g.preview.height || 1}"
                aria-label={g.title || 'GIF'}
                onclick={() => onpick(g)}
              >
                <img src={g.preview.url} alt="" loading="lazy" decoding="async" />
              </button>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
    {#if loading}
      <div class="state"><span class="ring" aria-label="Loading GIFs"></span></div>
    {/if}
  </div>

  <div class="foot">Powered by KLIPY</div>
</div>

<style>
  /* To the style guide: the popover card, tiles with a 1px line border and radius-md that
     change in 120ms, the meta type for titles and the attribution. */
  .panel {
    display: flex;
    flex-direction: column;
    width: 384px;
    height: 420px;
  }
  .head {
    padding: var(--space-2);
    border-bottom: 1px solid var(--line);
  }
  .search {
    width: 100%;
    height: 36px;
    background: var(--surface-200);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }
  h3 {
    margin: 0;
    padding: var(--space-2) var(--space-1) var(--space-1);
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.02em;
  }
  .columns {
    display: flex;
    gap: var(--space-2);
  }
  .col {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .tile {
    display: block;
    width: 100%;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-200);
    cursor: pointer;
    transition: border-color var(--dur-fast) var(--ease-out);
  }
  .tile:hover {
    border-color: var(--ink-muted);
  }
  .tile img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .state {
    display: grid;
    place-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-4);
    color: var(--ink-muted);
    text-align: center;
  }
  .state p {
    margin: 0;
  }
  .ring {
    width: 24px;
    height: 24px;
    box-sizing: border-box;
    border: 2px solid var(--line);
    border-top-color: var(--brand);
    border-radius: 50%;
    animation: spin 840ms linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex: none;
    height: 32px;
    padding: 0 var(--space-3);
    background: var(--surface-200);
    border-top: 1px solid var(--line);
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.02em;
  }
  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation: none;
    }
  }
</style>
