<script lang="ts">
  // The emoji panel: search, skin tone, a category rail, and every emoji in groups
  // (Emojibase 17). The reaction picker and the composer picker use it.
  // The list is virtual: about 1900 emoji need only the rows near the view in the DOM.
  import { onMount, tick } from 'svelte';
  import Apple from 'lucide-svelte/icons/apple';
  import Clock from 'lucide-svelte/icons/clock';
  import Flag from 'lucide-svelte/icons/flag';
  import Heart from 'lucide-svelte/icons/heart';
  import Lamp from 'lucide-svelte/icons/lamp';
  import Leaf from 'lucide-svelte/icons/leaf';
  import Plane from 'lucide-svelte/icons/plane';
  import Smile from 'lucide-svelte/icons/smile';
  import Trophy from 'lucide-svelte/icons/trophy';
  import Users from 'lucide-svelte/icons/users';
  import type { ComponentType } from 'svelte';
  import Emoji from './Emoji.svelte';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import {
    emojiEntry,
    emojiNow,
    loadEmoji,
    searchEmoji,
    withTone,
    type EmojiEntry,
    type EmojiGroup
  } from './emojidata';
  import { topReactions } from './reactions';

  let { onpick, label = 'Pick an emoji' }: { onpick: (emoji: string) => void; label?: string } =
    $props();

  const COLUMNS = 8;
  // Row heights in px. They match the styles below: the virtual list needs fixed rows.
  const HEAD_H = 28;
  const CELL_H = 40;
  const NONE_H = 40;
  // How far past the view to render, so that a fast scroll shows no blank rows.
  const OVERSCAN = 400;
  // The most rows to keep in the DOM, 60 rows are 480 emoji. A longer span starts again.
  const MAX_ROWS = 60;

  type Row =
    | { key: string; top: number; height: number; kind: 'head'; id: string; label: string }
    | { key: string; top: number; height: number; kind: 'none' }
    | { key: string; top: number; height: number; kind: 'cells'; items: EmojiEntry[] };
  type RowInit = Row extends infer R ? (R extends Row ? Omit<R, 'top'> : never) : never;
  const TONE_KEY = 'chord.skinTone';
  const ICONS: Record<string, ComponentType> = {
    frequent: Clock,
    smileys: Smile,
    people: Users,
    nature: Leaf,
    food: Apple,
    travel: Plane,
    activities: Trophy,
    objects: Lamp,
    symbols: Heart,
    flags: Flag
  };
  const TONES = ['✋', '✋🏻', '✋🏼', '✋🏽', '✋🏾', '✋🏿'];

  // The data loads in idle time after the start, so it is ready on most opens.
  let groups = $state<EmojiGroup[]>(emojiNow() ?? []);
  let query = $state('');
  let tone = $state(readTone());
  let choosingTone = $state(false);
  let hovered = $state<{ emoji: string; label: string } | null>(null);
  let active = $state('frequent');
  let scroller = $state<HTMLDivElement>();
  let search = $state<HTMLInputElement>();
  let scrollTop = $state(0);
  let viewHeight = $state(320);

  function readTone(): number {
    try {
      const n = Number(localStorage.getItem(TONE_KEY));
      return Number.isInteger(n) && n >= 0 && n <= 5 ? n : 0;
    } catch {
      return 0;
    }
  }

  function setTone(n: number) {
    tone = n;
    choosingTone = false;
    try {
      localStorage.setItem(TONE_KEY, String(n));
    } catch {
      /* storage blocked: keep it for this session */
    }
    search?.focus();
  }

  onMount(() => {
    if (!groups.length) void loadEmoji().then((g) => (groups = g));
    // Without scroll: the picker is near the edge of the window and the focus must not move it.
    search?.focus({ preventScroll: true });
    viewHeight = scroller?.clientHeight || viewHeight;
    return () => clearTimeout(termTimer);
  });

  // "Frequently used": the emoji that the user reacts with and types most.
  const frequent = $derived.by((): EmojiEntry[] =>
    topReactions(app.reactionUse, 16).map(
      (emoji) => emojiEntry(emoji) ?? { emoji, label: emoji, words: '', skins: null }
    )
  );
  // The search text that the list uses. It trails the input by a short time, so that fast
  // typing draws the emoji of the last key only.
  let term = $state('');
  let termTimer: ReturnType<typeof setTimeout> | undefined;
  const sections = $derived(
    term.trim()
      ? [{ id: 'results', label: 'Search results', emoji: searchEmoji(groups, term) }]
      : [{ id: 'frequent', label: 'Frequently used', emoji: frequent }, ...groups]
  );

  // Flat rows with fixed heights: a title, then the emoji in rows of 8.
  const layout = $derived.by(() => {
    const rows: Row[] = [];
    const tops: Record<string, number> = {};
    let top = 0;
    const add = (r: RowInit) => {
      rows.push({ ...r, top } as Row);
      top += r.height;
    };
    for (const s of sections) {
      tops[s.id] = top;
      add({ key: `h:${s.id}`, height: HEAD_H, kind: 'head', id: s.id, label: s.label });
      if (s.emoji.length === 0) add({ key: `n:${s.id}`, height: NONE_H, kind: 'none' });
      for (let i = 0; i < s.emoji.length; i += COLUMNS) {
        add({
          key: `r:${s.id}:${i}`,
          height: CELL_H,
          kind: 'cells',
          items: s.emoji.slice(i, i + COLUMNS)
        });
      }
    }
    return { rows, tops, height: top };
  });

  // The rows in or near the view. A binary search finds the first one. A row that showed
  // once stays in the DOM until the list changes or the view jumps, so that a scroll
  // back costs nothing. Making an emoji image is the slow part, not drawing it.
  let shown: { rows: Row[]; from: number; to: number } = { rows: [], from: 0, to: -1 };
  function windowOf(): [number, number] {
    const { rows } = layout;
    const from = scrollTop - OVERSCAN;
    const to = scrollTop + viewHeight + OVERSCAN;
    let lo = 0;
    let hi = rows.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (rows[mid].top + rows[mid].height < from) lo = mid + 1;
      else hi = mid;
    }
    let end = lo;
    while (end < rows.length && rows[end].top <= to) end++;
    return [lo, end];
  }
  const visible = $derived.by(() => {
    const { rows } = layout;
    const [lo, end] = windowOf();
    // A jump far away starts a new span: rows in between must not mount.
    if (shown.rows !== rows || shown.to < shown.from || end < shown.from || lo > shown.to || shown.to - shown.from > MAX_ROWS) {
      shown = { rows, from: lo, to: end };
    } else shown = { rows, from: Math.min(shown.from, lo), to: Math.max(shown.to, end) };
    return rows.slice(shown.from, shown.to);
  });

  function pick(e: EmojiEntry) {
    const emoji = withTone(e, tone);
    app.countEmoji(emoji);
    onpick(emoji);
  }

  async function jump(id: string) {
    query = '';
    settle();
    await tick();
    if (!scroller) return;
    scroller.scrollTop = layout.tops[id] ?? 0;
    scrollTop = scroller.scrollTop;
    active = id;
  }

  // The rail follows the scroll: the active group is the last one whose top passed.
  function scrolled() {
    if (!scroller) return;
    scrollTop = scroller.scrollTop;
    if (term) return;
    let current = 'frequent';
    for (const [id, top] of Object.entries(layout.tops)) {
      if (top <= scrollTop + 8) current = id;
    }
    active = current;
  }

  function onsearch() {
    clearTimeout(termTimer);
    // Clearing the text is at once. Typing waits 70ms for the next key.
    if (!query.trim()) settle();
    else termTimer = setTimeout(settle, 70);
  }

  function settle() {
    clearTimeout(termTimer);
    term = query;
    if (scroller) scroller.scrollTop = 0;
    scrollTop = 0;
  }

  function cells(): HTMLButtonElement[] {
    return [...(scroller?.querySelectorAll<HTMLButtonElement>('.cell') ?? [])];
  }

  function gridKey(e: KeyboardEvent) {
    const list = cells();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    const move: Record<string, number> = {
      ArrowRight: 1,
      ArrowLeft: -1,
      ArrowDown: COLUMNS,
      ArrowUp: -COLUMNS
    };
    if (e.key in move) {
      e.preventDefault();
      const next = i + move[e.key];
      if (next < 0) search?.focus();
      else list[Math.min(list.length - 1, next)]?.focus();
    }
  }

  function searchKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      cells()[0]?.focus();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      settle();
      const first = sections[0]?.emoji[0];
      if (term.trim() && first) pick(first);
    }
  }
</script>

<div class="panel" role="dialog" aria-label={label}>
  <div class="head">
    <input
      bind:this={search}
      bind:value={query}
      class="input search"
      type="search"
      placeholder="Find an emoji"
      aria-label="Find an emoji"
      autocomplete="off"
      spellcheck="false"
      oninput={onsearch}
      onkeydown={searchKey}
    />
    <div class="tone">
      <button
        class="tone-button"
        aria-label="Skin tone"
        aria-expanded={choosingTone}
        onclick={() => (choosingTone = !choosingTone)}><Emoji emoji={TONES[tone]} /></button
      >
      {#if choosingTone}
        <div class="tones" role="radiogroup" aria-label="Skin tone">
          {#each TONES as t, n (n)}
            <button
              role="radio"
              aria-checked={tone === n}
              aria-label={n === 0 ? 'Default skin tone' : `Skin tone ${n}`}
              class:on={tone === n}
              onclick={() => setTone(n)}><Emoji emoji={t} /></button
            >
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="body">
    <nav class="rail" aria-label="Emoji groups">
      {#each ['frequent', ...groups.map((g) => g.id)] as id (id)}
        <button
          class:on={active === id && !term}
          aria-label={id === 'frequent' ? 'Frequently used' : groups.find((g) => g.id === id)?.label}
          aria-current={active === id && !term ? 'true' : undefined}
          onclick={() => jump(id)}
        >
          <Icon icon={ICONS[id]} size={18} />
        </button>
      {/each}
    </nav>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="scroll" bind:this={scroller} onscroll={scrolled} onkeydown={gridKey}>
      {#if groups.length === 0}
        <div class="loading" aria-label="Loading the emoji"><span class="ring"></span></div>
      {:else}
        <div class="rows" style:height="{layout.height}px">
          {#each visible as r (r.key)}
            {#if r.kind === 'head'}
              <h3 class="row" style:transform="translateY({r.top}px)" data-section={r.id}>
                {r.label}
              </h3>
            {:else if r.kind === 'none'}
              <p class="row none" style:transform="translateY({r.top}px)">No emoji found.</p>
            {:else}
              <div class="row grid" style:transform="translateY({r.top}px)" role="group">
                {#each r.items as e (e.emoji)}
                  <button
                    class="cell"
                    aria-label={e.label}
                    onclick={() => pick(e)}
                    onpointerenter={() => (hovered = { emoji: withTone(e, tone), label: e.label })}
                    onfocus={() => (hovered = { emoji: withTone(e, tone), label: e.label })}
                    ><Emoji emoji={withTone(e, tone)} /></button
                  >
                {/each}
              </div>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="foot" aria-hidden="true">
    {#if hovered}
      <span class="big"><Emoji emoji={hovered.emoji} /></span>
      <span class="name">{hovered.label}</span>
    {:else}
      <span class="name">Pick an emoji</span>
    {/if}
  </div>
</div>

<style>
  /* To the style guide: the panel is a card (the popover gives surface-300, the line
     border and the corner), 40px cells with a radius-md hover in 120ms, section titles in
     the meta type, Lucide icons at a 1.5px line. */
  .panel {
    display: flex;
    flex-direction: column;
    width: 384px;
    height: 420px;
  }
  .head {
    position: relative;
    display: flex;
    gap: var(--space-2);
    padding: var(--space-2);
    border-bottom: 1px solid var(--line);
  }
  .search {
    flex: 1;
    min-width: 0;
    height: 36px;
    background: var(--surface-200);
  }
  .tone {
    position: relative;
  }
  .tone-button,
  .tones button {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    font-size: 20px;
    line-height: 1;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .tone-button:hover,
  .tones button:hover {
    background: var(--hover);
  }
  .tones {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 2;
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .tones button.on {
    background: var(--selected);
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .rail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: none;
    width: 44px;
    padding: var(--space-1) 0;
    align-items: center;
    background: var(--surface-200);
    border-right: 1px solid var(--line);
    overflow-y: auto;
  }
  .rail button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .rail button:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .rail button.on {
    background: var(--selected);
    color: var(--ink);
  }

  .scroll {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 0 var(--space-2) var(--space-2);
  }
  .rows {
    position: relative;
  }
  .row {
    /* A row that is off the screen skips layout and paint. */
    content-visibility: auto;
    contain-intrinsic-size: auto 40px;
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    box-sizing: border-box;
  }
  h3 {
    margin: 0;
    height: 28px;
    padding: var(--space-2) var(--space-1) var(--space-1);
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.02em;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(8, 40px);
  }
  .cell {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    /* The emoji image is 1.375em: 30px in a 40px cell. */
    font-size: 22px;
    line-height: 40px;
    text-align: center;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .cell:hover,
  .cell:focus-visible {
    background: var(--hover);
  }
  .cell:focus-visible {
    outline-offset: -2px;
  }
  .none {
    margin: 0;
    height: 40px;
    padding: var(--space-2) var(--space-1);
    color: var(--ink-muted);
  }
  .loading {
    display: grid;
    place-items: center;
    height: 100%;
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
    gap: var(--space-2);
    flex: none;
    height: 48px;
    padding: 0 var(--space-3);
    background: var(--surface-200);
    border-top: 1px solid var(--line);
  }
  .big {
    font-size: 28px;
    line-height: 1;
  }
  .name {
    overflow: hidden;
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    line-height: 16px;
    letter-spacing: 0.02em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (prefers-reduced-motion: reduce) {
    .ring,
    .tones {
      animation: none;
    }
  }
</style>
