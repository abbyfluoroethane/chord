<script lang="ts">
  // Add a theme from a link, or paste it. A theme is CSS with a comment header (@name,
  // @author, @mode, @accent), as on Vencord. A linked theme updates at each launch.
  import { theme } from '$lib/theme/theme.svelte';
  import { parseTheme } from '$lib/theme/themecss';
  import { sanitizeThemeCss } from '$lib/theme/themesafe';
  import Modal from './Modal.svelte';
  import Segmented from './Segmented.svelte';
  import { ui } from './ui.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let tab = $state<'link' | 'paste'>('link');
  let url = $state('');
  let css = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  const info = $derived(css.trim() ? parseTheme(css) : null);
  const blocked = $derived(css.trim() ? sanitizeThemeCss(css).blocked.length : 0);

  function done(r: ReturnType<typeof theme.import>) {
    if (!r.ok) {
      error = r.error;
      return;
    }
    ui.say(`Added ${r.theme.info.name}. It is now your ${r.theme.info.mode} theme.`);
    onclose();
  }

  async function addLink() {
    if (busy || !url.trim()) return;
    busy = true;
    error = null;
    try {
      done(await theme.importLink(url));
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Import a theme" {onclose} size="medium">
  <div class="tabs">
    <Segmented
      label="How to add the theme"
      value={tab}
      options={[
        { value: 'link', label: 'From a link' },
        { value: 'paste', label: 'Paste CSS' }
      ]}
      onchange={(v) => {
        tab = v;
        error = null;
      }}
    />
  </div>
  {#if tab === 'link'}
    <p class="note">
      Give the link of a CSS file. A GitHub page link works too. Chord checks the link each
      time it starts. If the file changed, Chord asks before it uses the new version.
    </p>
    <form
      class="row"
      onsubmit={(e) => {
        e.preventDefault();
        void addLink();
      }}
    >
      <input
        class="input"
        type="url"
        bind:value={url}
        oninput={() => (error = null)}
        placeholder="https://github.com/user/repo/blob/main/theme.css"
        spellcheck="false"
        autocomplete="off"
        aria-label="Theme link"
      />
      <button class="btn btn-primary" type="submit" disabled={busy || !url.trim()}>
        {busy ? 'Adding…' : 'Add'}
      </button>
    </form>
  {:else}
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
        {#if blocked}This CSS asks for {blocked} {blocked === 1 ? 'request' : 'requests'} to other
          hosts. Chord blocks {blocked === 1 ? 'it' : 'them'}.{/if}
      </p>
    {/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <p class="note small">
    A theme can change how any part of Chord looks. Chord blocks every request from a theme to
    another host, but import only CSS from people you trust.
  </p>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    {#if tab === 'paste'}
      <button class="btn btn-primary" disabled={!css.trim()} onclick={() => done(theme.import(css))}
        >Add theme</button
      >
    {/if}
  {/snippet}
</Modal>

<style>
  .note {
    margin: 0 0 var(--space-3);
    color: var(--ink-muted);
  }
  .tabs {
    margin-bottom: var(--space-3);
  }
  .row {
    display: flex;
    gap: var(--space-2);
  }
  .row .input {
    flex: 1;
    min-width: 0;
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
