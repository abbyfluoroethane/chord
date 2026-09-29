<script lang="ts">
  import SmilePlus from 'lucide-svelte/icons/smile-plus';
  import EmojiPicker from './EmojiPicker.svelte';
  import Icon from './Icon.svelte';
  import { tooltip } from './tooltip';
  import type { Reaction } from './types';

  let {
    reactions,
    ontoggle
  }: { reactions: Reaction[]; ontoggle: (emoji: string) => void } = $props();

  let addBtn = $state<HTMLButtonElement>();
  let picking = $state(false);
</script>

{#if reactions.length}
  <div class="pills">
    {#each reactions as r (r.emoji)}
      <button
        class="pill"
        class:mine={r.mine}
        aria-pressed={r.mine}
        aria-label="{r.emoji} {r.count}, {r.mine ? 'remove your reaction' : 'react too'}"
        onclick={() => ontoggle(r.emoji)}
      >
        <span class="emoji">{r.emoji}</span>
        <span class="count">{r.count}</span>
      </button>
    {/each}
    <button
      bind:this={addBtn}
      class="pill add"
      aria-label="Add reaction"
      use:tooltip={{ text: 'Add reaction', side: 'top' }}
      onclick={() => (picking = !picking)}
    >
      <Icon icon={SmilePlus} size={16} />
    </button>
  </div>
  {#if picking && addBtn}
    <EmojiPicker anchor={addBtn} onpick={ontoggle} onclose={() => (picking = false)} />
  {/if}
{/if}

<style>
  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-300);
    color: var(--ink-muted);
    font-size: 13px;
    font-weight: 500;
    transition:
      border-color var(--dur-fast),
      background var(--dur-fast);
  }
  .pill:hover {
    border-color: var(--ink-muted);
  }
  .pill.mine {
    border-color: var(--brand);
    background: var(--brand-soft);
    color: var(--brand-ink);
  }
  .emoji {
    font-size: 14px;
    line-height: 1;
  }
  .add {
    padding: 0 var(--space-2);
    opacity: 0;
    transition: opacity var(--dur-fast);
  }
  .pills:hover .add,
  .pills:focus-within .add,
  .add:focus-visible {
    opacity: 1;
  }
</style>
