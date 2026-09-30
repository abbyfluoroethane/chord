<script lang="ts">
  // Paste a theme: CSS with a comment header (@name, @author, @mode, @accent), as on
  // Vencord. Chord reads the header as you paste and shows what it found.
  import { theme } from '$lib/theme/theme.svelte';
  import { parseTheme } from '$lib/theme/themecss';
  import Modal from './Modal.svelte';
  import { ui } from './ui.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let css = $state('');
  let error = $state<string | null>(null);
  const info = $derived(css.trim() ? parseTheme(css) : null);

  function add() {
    const r = theme.import(css);
    if (!r.ok) {
      error = r.error;
      return;
    }
    ui.say(`Added ${r.theme.info.name}. It is now your ${r.theme.info.mode} theme.`);
    onclose();
  }
</script>

<Modal title="Import a theme" {onclose} size="medium">
  <p class="note">
    Paste the CSS of a theme. A comment header can give <code>@name</code>, <code>@author</code>,
    <code>@mode</code> (dark or light) and one <code>@accent</code> line for each accent.
  </p>
  <textarea
    class="input css"
    bind:value={css}
    oninput={() => (error = null)}
    placeholder={'/**\n * @name My theme\n * @mode dark\n */\n:root {\n  --surface-100: #1e1e2e;\n}'}
    spellcheck="false"
    aria-label="Theme CSS"
  ></textarea>
  {#if info}
    <p class="found" role="status">
      <b>{info.name}</b>{info.author ? ` by ${info.author}` : ''}: a {info.mode} theme with
      {info.accents.length === 1 ? '1 accent' : `${info.accents.length} accents`}.
    </p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <p class="note small">
    A theme can change how any part of Chord looks. Import only CSS from people you trust.
  </p>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={!css.trim()} onclick={add}>Add theme</button>
  {/snippet}
</Modal>

<style>
  .note {
    margin: 0 0 var(--space-3);
    color: var(--ink-muted);
  }
  .small {
    margin: var(--space-3) 0 0;
    font-size: 12px;
    line-height: 16px;
  }
  .css {
    width: 100%;
    height: 240px;
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 20px;
    resize: vertical;
  }
  .found {
    margin: var(--space-2) 0 0;
  }
  .error {
    margin: var(--space-2) 0 0;
    color: var(--danger);
  }
</style>
