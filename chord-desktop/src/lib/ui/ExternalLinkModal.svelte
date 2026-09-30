<script lang="ts">
  // "Leave Chord?" for a masked link that shows one address and opens another. It names the
  // real host, in Punycode form, and warns about look-alike letters. The link opens only on
  // a click of the main button.
  import Modal from './Modal.svelte';
  import { hostWarning, linkHost } from './linkguard';

  let { href, text, onclose }: { href: string; text: string; onclose: () => void } = $props();

  const host = $derived(linkHost(href) ?? href);
  const warning = $derived(hostWarning(host));
  // The address with its host in Punycode, so a look-alike letter cannot hide in it.
  const shown = $derived.by(() => {
    try {
      return new URL(href).href;
    } catch {
      return href;
    }
  });
</script>

<Modal title="Leave Chord?" {onclose}>
  <p class="lead">This link opens <strong class="host">{host}</strong>.</p>
  <p class="shown">The message shows it as “{text}”.</p>
  <p class="address">{shown}</p>
  {#if warning}<p class="note bad">{warning}</p>{/if}
  <p class="note">Open it only if you trust the person who sent it.</p>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <a class="btn btn-primary" {href} target="_blank" rel="noopener noreferrer" onclick={onclose}>Open link</a>
  {/snippet}
</Modal>

<style>
  .lead,
  .shown {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .shown {
    margin-top: var(--space-2);
    color: var(--ink-muted);
  }
  .address {
    margin: var(--space-3) 0 0;
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--ink-muted);
  }
  .note {
    margin: var(--space-3) 0 0;
    color: var(--ink-muted);
  }
  .note.bad {
    color: var(--danger);
  }
</style>
