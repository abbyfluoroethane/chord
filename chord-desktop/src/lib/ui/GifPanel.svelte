<script lang="ts">
  // The GIF panel: KLIPY search in two columns, trending before a search. A pick sends the
  // GIF as a link with an embed (XEP-0066). KLIPY asks for "Search KLIPY" in the field
  // and "Powered by KLIPY" on the panel.
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import type { Gif } from '$lib/chord';
  import * as fx from '$lib/fixtures/data';
  import { plainError } from './adapt';
  import { api, live } from './bridge';
  import { mergePage, splitColumns } from './gif-layout';

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
  // The slugs of the tiles that are on screen or near it. Only these tiles hold an image.
  // A GIF that is far off screen has no image, so it does not decode and does not animate.
  const near = new SvelteSet<string>();
  const watching = new Map<Element, string>();
  let seen: IntersectionObserver | undefined;
  let far: IntersectionObserver | undefined;
  let more: IntersectionObserver | undefined;
  let end = $state<HTMLDivElement>();

  /**
   * The preview has no API key. It searches a snapshot of real KLIPY results if the
   * published preview has one (preview-gifs.json, never in git), else the sample GIFs.
   */
  type SampleGif = Gif & { tags?: string[] };
  let samples: Promise<SampleGif[]> | null = null;
  function loadSamples(): Promise<SampleGif[]> {
    samples ??= fetch('preview-gifs.json')
      .then((r) => (r.ok ? (r.json() as Promise<SampleGif[]>) : fx.gifs))
      .catch(() => fx.gifs);
    return samples;
  }

  async function fetchPage(q: string, n: number): Promise<{ items: Gif[]; hasNext: boolean }> {
    if (!live) {
      const all = await loadSamples();
      const t = q.trim().toLowerCase();
      const list = t
        ? all.filter((g) => [g.title, ...(g.tags ?? [])].join(' ').toLowerCase().includes(t))
        : all;
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
      items = mergePage(reset ? [] : items, result.items);
      page = n;
      hasNext = result.hasNext;
      if (reset && scroller) scroller.scrollTop = 0;
    } catch (e) {
      if (id !== request) return;
      const code = (e as { code?: string } | null)?.code;
      error =
        code === 'gifUnavailable'
          ? {
              text: 'This build of Chord has no KLIPY key, so GIF search is off. The file docs/gifs.md tells you how to add one.',
              retry: false
            }
          : { text: plainError(e), retry: true };
    } finally {
      if (id === request) loading = false;
    }
  }

  function typed() {
    clearTimeout(timer);
    timer = setTimeout(() => void load(true), DEBOUNCE_MS);
  }

  onMount(() => {
    // Both observers use the scroll box as the root. They replace a scroll handler, so
    // a scroll does no work in script.
    seen = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const slug = watching.get(e.target);
          if (slug === undefined) continue;
          if (e.isIntersecting) near.add(slug);
        }
      },
      { root: scroller, rootMargin: '300px 0px' }
    );
    // A tile gives up its image only when it is far away. This stops a reload when the
    // user scrolls back and forth.
    far = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          const slug = watching.get(e.target);
          if (slug !== undefined && !e.isIntersecting) near.delete(slug);
        }
      },
      { root: scroller, rootMargin: '1500px 0px' }
    );
    more = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) nextPage();
      },
      { root: scroller, rootMargin: '0px 0px 300px 0px' }
    );
    for (const el of watching.keys()) {
      seen.observe(el);
      far.observe(el);
    }
    void load(true);
    search?.focus();
    return () => {
      clearTimeout(timer);
      seen?.disconnect();
      far?.disconnect();
      more?.disconnect();
    };
  });

  // Load the next page when the end of the list comes near.
  function nextPage() {
    if (!loading && hasNext && !error) void load(false);
  }

  // Watch the end marker. The observer reports again when the page state changes, so a
  // page that leaves the marker on screen loads the next one.
  $effect(() => {
    void items.length;
    void loading;
    if (!end || !more) return;
    const marker = end;
    const watcher = more;
    watcher.observe(marker);
    return () => watcher.unobserve(marker);
  });

  /** Tell the panel when a tile comes near the screen and when it leaves. */
  function track(node: HTMLElement, slug: string) {
    watching.set(node, slug);
    seen?.observe(node);
    far?.observe(node);
    return {
      destroy() {
        watching.delete(node);
        seen?.unobserve(node);
        far?.unobserve(node);
        near.delete(slug);
      }
    };
  }

  const columns = $derived(splitColumns(items));

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

  <div class="scroll" bind:this={scroller}>
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
                use:track={g.slug}
                onclick={() => onpick(g)}
              >
                {#if near.has(g.slug)}
                  <img src={g.preview.url} alt="" decoding="async" draggable="false" />
                {/if}
              </button>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
    <div class="end" bind:this={end}></div>
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
  .end {
    height: 1px;
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
