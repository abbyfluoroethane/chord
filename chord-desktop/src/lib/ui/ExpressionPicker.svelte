<script lang="ts">
  // The composer picker: GIFs and emoji in one popover with two tabs, above the message
  // box. The GIF tab goes away when the user turns the GIF picker off in Privacy.
  import type { Gif } from '$lib/chord';
  import EmojiPanel from './EmojiPanel.svelte';
  import GifPanel from './GifPanel.svelte';
  import Popover from './Popover.svelte';
  import { prefs } from './prefs.svelte';

  let {
    anchor,
    tab = $bindable('emoji'),
    onemoji,
    ongif,
    onclose
  }: {
    anchor: HTMLElement;
    tab?: 'gif' | 'emoji';
    onemoji: (emoji: string) => void;
    ongif: (gif: Gif) => void;
    onclose: () => void;
  } = $props();

  const shown = $derived(prefs.gifPicker ? tab : 'emoji');
</script>

<Popover {anchor} {onclose} placement="top-end" label="GIFs and emoji">
  {#if prefs.gifPicker}
    <div class="tabs" role="tablist" aria-label="GIFs and emoji">
      <button role="tab" aria-selected={shown === 'gif'} onclick={() => (tab = 'gif')}>GIFs</button>
      <button role="tab" aria-selected={shown === 'emoji'} onclick={() => (tab = 'emoji')}>
        Emoji
      </button>
    </div>
  {/if}
  {#if shown === 'gif'}
    <GifPanel onpick={ongif} />
  {:else}
    <EmojiPanel onpick={onemoji} />
  {/if}
</Popover>

<style>
  .tabs {
    display: flex;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-2) 0;
  }
  .tabs button {
    height: 32px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    font-size: 14px;
    font-weight: 500;
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .tabs button:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .tabs button[aria-selected='true'] {
    background: var(--selected);
    color: var(--ink);
  }
</style>
