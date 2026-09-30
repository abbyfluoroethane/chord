<script lang="ts">
  // The emoji panel: search, skin tone, a category rail, and every emoji in groups
  // (Emojibase 17). The reaction picker and the composer picker use it.
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

  let groups = $state<EmojiGroup[]>([]);
  let query = $state('');
  let tone = $state(readTone());
  let choosingTone = $state(false);
  let hovered = $state<{ emoji: string; label: string } | null>(null);
  let active = $state('frequent');
  let scroller = $state<HTMLDivElement>();
  let search = $state<HTMLInputElement>();

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
    void loadEmoji().then((g) => (groups = g));
    search?.focus();
  });

  // "Frequently used": the emoji that the user reacts with and types most.
  const frequent = $derived.by((): EmojiEntry[] => {
    const all = new Map(groups.flatMap((g) => g.emoji.map((e) => [e.emoji, e] as const)));
    return topReactions(app.reactionUse, 16)
      .map((emoji) => all.get(emoji) ?? { emoji, label: emoji, words: '', skins: null });
  });
  const sections = $derived(
    query.trim()
      ? [{ id: 'results', label: 'Search results', emoji: searchEmoji(groups, query) }]
      : [{ id: 'frequent', label: 'Frequently used', emoji: frequent }, ...groups]
  );

  function pick(e: EmojiEntry) {
    const emoji = withTone(e, tone);
    app.countEmoji(emoji);
    onpick(emoji);
  }

  async function jump(id: string) {
    query = '';
    await tick();
    const target = scroller?.querySelector<HTMLElement>(`[data-section="${id}"]`);
    if (scroller && target) scroller.scrollTop = target.offsetTop - scroller.offsetTop;
    active = id;
  }

  // The rail follows the scroll: the active group is the last one whose top passed.
  function scrolled() {
    if (!scroller || query) return;
    const top = scroller.scrollTop + scroller.offsetTop + 8;
    let current = 'frequent';
    for (const el of scroller.querySelectorAll<HTMLElement>('[data-section]')) {
      if (el.offsetTop <= top) current = el.dataset.section ?? current;
    }
    active = current;
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
      const first = sections[0]?.emoji[0];
      if (query.trim() && first) pick(first);
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
          class:on={active === id && !query}
          aria-label={id === 'frequent' ? 'Frequently used' : groups.find((g) => g.id === id)?.label}
          aria-current={active === id && !query ? 'true' : undefined}
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
        {#each sections as s (s.id)}
          <section data-section={s.id} aria-label={s.label}>
            <h3>{s.label}</h3>
            {#if s.emoji.length === 0}
              <p class="none">No emoji found.</p>
            {:else}
              <div class="grid">
                {#each s.emoji as e (e.emoji)}
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
          </section>
        {/each}
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
  section {
    /* Offscreen groups skip layout and paint until they scroll near. */
    content-visibility: auto;
    contain-intrinsic-size: auto 400px;
  }
  h3 {
    position: sticky;
    top: 0;
    z-index: 1;
    margin: 0;
    padding: var(--space-2) var(--space-1) var(--space-1);
    background: var(--surface-300);
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
    font-size: 26px;
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
    margin: var(--space-2) var(--space-1);
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
