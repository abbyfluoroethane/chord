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
    bare = false,
    children
  }: {
    title: string;
    onclose: () => void;
    size?: 'small' | 'medium';
    footer?: Snippet;
    /** Replaces the title text in the header. The title stays the accessible name. */
    heading?: Snippet;
    /** No title row and no padding. The content fills the dialog. The close button floats on top. */
    bare?: boolean;
    children: Snippet;
  } = $props();

  let dlg = $state<HTMLDialogElement>();

  $effect(() => {
    if (!dlg) return;
    const opener = document.activeElement as HTMLElement | null;
    dlg.showModal();
    startFocus(dlg);
    untrack(() => (ui.overlays += 1));
    return () => {
      untrack(() => (ui.overlays -= 1));
      // The opener can be gone, for example a menu item. Then the focus stays where it is.
      if (opener?.isConnected) opener.focus?.();
    };
  });

  // The browser puts the focus on the Close button. Move it to the first field of the form.
  // A dialog without a field gets the safe button: Cancel for a danger button, else the main one.
  // A child that sets its own focus later still wins.
  function startFocus(d: HTMLDialogElement) {
    const field = d.querySelector<HTMLElement>(
      '.body input:not([type=hidden]):not([disabled]), .body textarea:not([disabled]), .body select:not([disabled])'
    );
    if (field) {
      field.focus();
      return;
    }
    const buttons = [...d.querySelectorAll<HTMLElement>('footer button:not([disabled]), footer a')];
    const main = buttons.find((b) => b.classList.contains('btn-primary')) ?? buttons.at(-1);
    const danger = main?.classList.contains('btn-danger');
    (danger ? buttons[0] : main)?.focus();
  }

  function backdrop(e: MouseEvent) {
    if (e.target === dlg) onclose();
  }

  function cancel(e: Event) {
    e.preventDefault();
    onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dlg} class={[size, bare && 'bare']} aria-label={title} onclick={backdrop} oncancel={cancel}>
  <header>
    {#if heading}
      {@render heading()}
    {:else if !bare}
      <h2 class="title">{title}</h2>
    {/if}
    <button class="close" aria-label="Close" title="Close" onclick={onclose}><Icon icon={X} size={18} /></button>
  </header>
  <div class="body">{@render children()}</div>
  {#if footer}<footer data-guard>{@render footer()}</footer>{/if}
</dialog>

<style>
  dialog {
    padding: 0;
    color: var(--ink);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    max-height: min(660px, calc(100vh - 64px));
    animation: pop var(--dur-arrive) var(--ease-out);
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
  dialog.bare {
    overflow: hidden;
  }
  dialog.bare header {
    position: absolute;
    top: var(--space-2);
    right: var(--space-2);
    z-index: 1;
    padding: 0;
  }
  dialog.bare .close {
    background: color-mix(in srgb, black 45%, transparent);
    color: white;
  }
  dialog.bare .close:hover {
    background: color-mix(in srgb, black 65%, transparent);
    color: white;
  }
  dialog.bare .body {
    padding: 0;
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
