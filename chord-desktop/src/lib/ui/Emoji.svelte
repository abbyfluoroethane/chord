<script lang="ts">
  // One emoji in the chosen pack. An emoji that the pack lacks falls back to Twemoji,
  // then to the system font. The alt text keeps copy, search and screen readers right.
  import { emojiPacks } from './emojipacks.svelte';

  let { emoji }: { emoji: string } = $props();

  // 0: the chosen pack, 1: Twemoji, 2: the system font.
  let step = $state(0);
  $effect(() => {
    void emoji;
    void emojiPacks.active;
    step = 0;
  });
  const src = $derived(
    step === 0 ? emojiPacks.url(emoji) : step === 1 ? emojiPacks.url(emoji, 'twemoji') : null
  );
</script>

{#if src}<img
    class="emoji"
    {src}
    alt={emoji}
    draggable="false"
    onerror={() => (step += 1)}
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
