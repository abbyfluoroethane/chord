<script lang="ts">
  // One emoji in the chosen pack. An emoji that the pack lacks falls back to Twemoji,
  // then to the system font. The alt text keeps copy, search and screen readers right.
  import { emojiPacks } from './emojipacks.svelte';

  import type { EmojiPackId } from './emojipackids';

  /** `pack` draws in one pack, for example a sample in the settings. */
  let { emoji, pack }: { emoji: string; pack?: EmojiPackId } = $props();

  // 0: the chosen pack, 1: Twemoji, 2: the system font.
  let step = $state(0);
  $effect(() => {
    void emoji;
    void emojiPacks.active;
    void pack;
    step = 0;
  });
  const src = $derived(
    step === 0 ? emojiPacks.url(emoji, pack ?? emojiPacks.active) : step === 1 ? emojiPacks.url(emoji, 'twemoji') : null
  );
</script>

{#if src}<img
    class="emoji"
    {src}
    alt={emoji}
    draggable="false"
    decoding="async"
    onerror={() => (step += 1)}
    onload={(e) => {
      if (!(e.currentTarget as HTMLImageElement).naturalWidth) step += 1;
    }}
  />{:else}{emoji}{/if}

<style>
  /* The emoji sits on the text line like a glyph: 1.375em, as Discord draws them. */
  .emoji {
    display: inline-block;
    width: 1.375em;
    height: 1.375em;
    margin: 0 0.05em;
    vertical-align: -0.3em;
    object-fit: contain;
  }
</style>
