<script lang="ts" module>
  // The images that failed to load, as "pack:emoji". All emoji share this one set, so an
  // emoji has no state or effect of its own. A failure is rare, so one counter is enough
  // to make every emoji check the set again.
  const failed = new Set<string>();
  let failures = $state(0);
</script>

<script lang="ts">
  // One emoji in the chosen pack. An emoji that the pack lacks falls back to Twemoji,
  // then to the system font. The alt text keeps copy, search and screen readers right.
  import { emojiPacks } from './emojipacks.svelte';

  import type { EmojiPackId } from './emojipackids';

  /** `pack` draws in one pack, for example a sample in the settings. */
  let { emoji, pack }: { emoji: string; pack?: EmojiPackId } = $props();

  // The pack that draws now, then Twemoji, then null: the system font.
  const draw = $derived.by((): { src: string; key: string } | null => {
    void failures;
    const chosen = pack ?? emojiPacks.active;
    for (const id of chosen === 'twemoji' ? [chosen] : [chosen, 'twemoji' as const]) {
      const key = `${id}:${emoji}`;
      if (failed.has(key)) continue;
      const src = emojiPacks.url(emoji, id);
      if (src) return { src, key };
    }
    return null;
  });

  function fail() {
    if (!draw) return;
    failed.add(draw.key);
    failures += 1;
  }
</script>

{#if draw}<img
    class="emoji"
    src={draw.src}
    alt={emoji}
    draggable="false"
    decoding="async"
    onerror={fail}
    onload={(e) => {
      if (!(e.currentTarget as HTMLImageElement).naturalWidth) fail();
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
