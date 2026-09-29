<script lang="ts">
  // An address in mono. A click copies it and a small "Copied" tip shows for a moment.
  let { address }: { address: string } = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function copy() {
    void navigator.clipboard?.writeText(address);
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), 1400);
  }
</script>

<span class="wrap">
  <button class="addr mono" aria-label="Copy address {address}" onclick={copy}>{address}</button>
  {#if copied}<span class="tip" role="status">Copied</span>{/if}
</span>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
    max-width: 100%;
  }
  .addr {
    color: var(--ink-muted);
    text-align: left;
    overflow-wrap: anywhere;
    border-radius: var(--radius-sm);
  }
  .addr:hover {
    color: var(--ink);
    text-decoration: underline;
  }
  .tip {
    position: absolute;
    left: 0;
    bottom: calc(100% + 4px);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--ink);
    color: var(--surface-100);
    font-size: 12px;
    line-height: 20px;
    font-weight: 500;
    white-space: nowrap;
    animation: arrive var(--dur-fast) var(--ease-out);
  }
</style>
