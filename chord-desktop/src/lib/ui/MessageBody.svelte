<script lang="ts">
  // Message text as segments: links, code, mentions. No {@html}.
  import { app } from './app.svelte';
  import { segments } from './richtext';

  let { body }: { body: string } = $props();

  const parts = $derived(
    segments(body, [app.me.name, app.me.address.split('@')[0], app.me.address])
  );
</script>

<span class="body"
  >{#each parts as p, i (i)}{#if p.t === 'text'}{p.v}{:else if p.t === 'code'}<code class="inline">{p.v}</code
      >{:else if p.t === 'link'}<a href={p.v} target="_blank" rel="noopener noreferrer">{p.v}</a
      >{:else}<span class="mention" class:me={p.me}>{p.v}</span>{/if}{/each}</span
>

<style>
  .body {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .inline {
    padding: 1px 4px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .mention {
    padding: 0 2px;
    border-radius: var(--radius-sm);
    background: var(--brand-soft);
    color: var(--brand-ink);
    font-weight: 500;
  }
</style>
