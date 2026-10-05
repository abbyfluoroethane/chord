<script lang="ts">
  // A key fingerprint in 8 blocks of 8 hex characters, in a mono font, with a copy button.
  import Copy from 'lucide-svelte/icons/copy';
  import Icon from './Icon.svelte';
  import { copyText } from './clipboard';
  import { fingerprintBlocks, fingerprintText } from './omemodata';
  import { tooltip } from './tooltip';

  let { fingerprint }: { fingerprint: string | null } = $props();

  const blocks = $derived(fingerprintBlocks(fingerprint));
</script>

{#if blocks.length}
  <div class="print">
    <code class="blocks" aria-label="Fingerprint">
      {#each blocks as b, i (i)}<span>{b}</span>{/each}
    </code>
    <button
      class="copy"
      aria-label="Copy the fingerprint"
      use:tooltip={{ text: 'Copy the fingerprint', side: 'top' }}
      onclick={() => void copyText(fingerprintText(fingerprint), 'Fingerprint copied.')}
    >
      <Icon icon={Copy} size={16} />
    </button>
  </div>
{:else}
  <span class="none">No key yet.</span>
{/if}

<style>
  .print {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
  }
  .blocks {
    display: grid;
    grid-template-columns: repeat(4, max-content);
    gap: 2px var(--space-3);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 18px;
    user-select: text;
  }
  .copy {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: none;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
  }
  .copy:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .none {
    color: var(--ink-muted);
    font-size: 13px;
  }
</style>
