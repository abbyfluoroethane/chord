<script lang="ts">
  import { untrack } from 'svelte';
  // Modal dialog on the native <dialog> element. It traps focus, handles
  // Esc, and returns focus to whatever opened it.
  import type { Snippet } from 'svelte';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import { ui } from './ui.svelte';

  let {
    title,
    onclose,
    size = 'small',
    footer,
    heading,
    children
  }: {
    title: string;
    onclose: () => void;
    size?: 'small' | 'medium';
    footer?: Snippet;
    /** Replaces the title text in the header. The title stays the accessible name. */
    heading?: Snippet;
    children: Snippet;
  } = $props();

  let dlg = $state<HTMLDialogElement>();

  $effect(() => {
    if (!dlg) return;
    const opener = document.activeElement as HTMLElement | null;
    dlg.showModal();
    untrack(() => (ui.overlays += 1));
    return () => {
      untrack(() => (ui.overlays -= 1));
      opener?.focus?.();
    };
  });

  function backdrop(e: MouseEvent) {
    if (e.target === dlg) onclose();
  }

  function cancel(e: Event) {
    e.preventDefault();
    onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dlg} class={size} aria-label={title} onclick={backdrop} oncancel={cancel}>
  <header>
    {#if heading}
      {@render heading()}
    {:else}
      <h2 class="title">{title}</h2>
    {/if}
    <button class="close" aria-label="Close" onclick={onclose}><Icon icon={X} size={18} /></button>
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</dialog>

<style>
  dialog {
    padding: 0;
    color: var(--ink);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    max-height: min(660px, calc(100vh - 64px));
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog.small {
    width: 440px;
  }
  dialog.medium {
    width: 600px;
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-4) var(--space-2) var(--space-6);
  }
  h2 {
    margin: 0;
  }
  .close {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
  }
  .close:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .body {
    padding: var(--space-2) var(--space-6) var(--space-6);
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-6);
    background: var(--surface-300);
    border-top: 1px solid var(--line);
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
  }
</style>
