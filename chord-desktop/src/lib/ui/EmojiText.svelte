<script lang="ts">
  // Plain text that can hold emoji. Each emoji shows in the emoji pack that the user
  // chose (Settings > Appearance). The markdown renderer uses it for every text run.
  import Emoji from './Emoji.svelte';
  import { splitEmoji } from './emojipacks.svelte';

  let { text }: { text: string } = $props();

  const parts = $derived(splitEmoji(text));
</script>

{#each parts as p, i (i)}{#if p.emoji}<Emoji emoji={p.text} />{:else}{p.text}{/if}{/each}
