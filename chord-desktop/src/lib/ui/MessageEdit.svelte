<script lang="ts">
  // Inline edit. Esc cancels, Enter saves, Shift+Enter adds a line.
  import { untrack } from 'svelte';
  import { app } from './app.svelte';
  import { loadShortcodes, mayHaveShortcode, replaceShortcodesOutsideCode } from './shortcodes';
  import type { TimelineItem } from './types';

  let { item }: { item: TimelineItem } = $props();

  let value = $state(untrack(() => item.body));
  let box = $state<HTMLTextAreaElement>();

  function fit() {
    if (!box) return;
    box.style.height = 'auto';
    box.style.height = `${Math.min(box.scrollHeight, 144)}px`;
  }

  $effect(() => {
    if (!box) return;
    box.focus();
    box.setSelectionRange(box.value.length, box.value.length);
    fit();
  });

  // As in the composer: other clients do not know :shortcodes:, so the edit saves real emoji.
  async function save() {
    const out = mayHaveShortcode(value)
      ? replaceShortcodesOutsideCode(value, await loadShortcodes())
      : value;
    app.edit(item.id, out);
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      app.editingId = null;
    } else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void save();
    }
  }
</script>

<div class="edit">
  <textarea
    bind:this={box}
    bind:value
    rows="1"
    aria-label="Edit message"
    oninput={fit}
    onkeydown={keydown}
  ></textarea>
  <p class="hint">
    escape to <button onclick={() => (app.editingId = null)}>cancel</button> · enter to
    <button onclick={save}>save</button>
  </p>
</div>

<style>
  textarea {
    display: block;
    width: 100%;
    max-height: 144px;
    padding: var(--space-2) var(--space-3);
    resize: none;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    line-height: 22px;
  }
  textarea:focus-visible {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }
  .hint {
    margin: var(--space-1) 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--ink-muted);
  }
  .hint button {
    color: var(--accent);
  }
  .hint button:hover {
    text-decoration: underline;
  }
</style>
