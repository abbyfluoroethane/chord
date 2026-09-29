<script lang="ts">
  import { untrack } from 'svelte';
  // Ctrl/Cmd+K. Fuzzy search over circles, channels, and DMs.
  import AtSign from 'lucide-svelte/icons/at-sign';
  import Circle from 'lucide-svelte/icons/circle-dot';
  import Hash from 'lucide-svelte/icons/hash';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { score } from './fuzzy';
  import { spaceKey } from './types';
  import { ui } from './ui.svelte';

  interface Hit {
    id: string;
    label: string;
    hint: string;
    kind: 'circle' | 'channel' | 'dm';
    go: () => void;
  }

  let dlg = $state<HTMLDialogElement>();
  let query = $state('');
  let index = $state(0);

  const all = $derived<Hit[]>([
    ...app.spaces.map((s) => ({
      id: `s:${spaceKey(s)}`,
      label: s.name,
      hint: 'Circle',
      kind: 'circle' as const,
      go: () => app.selectSpace(spaceKey(s))
    })),
    ...app.channels.map((c) => ({
      id: `c:${c.jid}`,
      label: c.name,
      hint: c.kind === 'dm' ? 'Direct message' : (app.spaceOf(c.space ?? '')?.name ?? ''),
      kind: c.kind === 'dm' ? ('dm' as const) : ('channel' as const),
      go: () => app.selectChannel(c.jid)
    }))
  ]);

  const hits = $derived.by(() => {
    let q = query.trim();
    let only: Hit['kind'] | null = null;
    if (q.startsWith('#')) [only, q] = ['channel', q.slice(1)];
    else if (q.startsWith('@')) [only, q] = ['dm', q.slice(1)];
    return all
      .filter((h) => !only || h.kind === only)
      .map((h) => ({ h, s: score(q, h.label) * (h.kind === 'circle' ? 1 : 1.05) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .slice(0, 10)
      .map((x) => x.h);
  });

  $effect(() => {
    void hits;
    index = 0;
  });

  $effect(() => {
    if (!dlg) return;
    dlg.showModal();
    untrack(() => (ui.overlays += 1));
    return () => {
      untrack(() => (ui.overlays -= 1));
    };
  });

  function close() {
    ui.switcherOpen = false;
  }

  function pick(h: Hit | undefined) {
    if (!h) return;
    close();
    h.go();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      index = hits.length ? (index + 1) % hits.length : 0;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      index = hits.length ? (index - 1 + hits.length) % hits.length : 0;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      pick(hits[index]);
    }
  }

  const icons = { circle: Circle, channel: Hash, dm: AtSign };
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dlg}
  aria-label="Quick switcher"
  onclick={(e) => e.target === dlg && close()}
  oncancel={(e) => {
    e.preventDefault();
    close();
  }}
>
  <input
    class="q"
    role="combobox"
    aria-expanded="true"
    aria-controls="switcher-list"
    aria-activedescendant={hits[index] ? `sw-${hits[index].id}` : undefined}
    aria-label="Where would you like to go?"
    placeholder="Where would you like to go?"
    autocomplete="off"
    bind:value={query}
    onkeydown={keydown}
  />
  <ul id="switcher-list" role="listbox" aria-label="Results">
    {#each hits as h, i (h.id)}
      <li
        id="sw-{h.id}"
        role="option"
        aria-selected={i === index}
        class:on={i === index}
        onpointermove={() => (index = i)}
        onclick={() => pick(h)}
      >
        <Icon icon={icons[h.kind]} size={16} />
        <span class="label">{h.label}</span>
        <span class="meta">{h.hint}</span>
      </li>
    {:else}
      <li class="none" role="presentation">Nothing matches. Try a shorter name.</li>
    {/each}
  </ul>
  <p class="tip meta">Start with # for channels or @ for DMs.</p>
</dialog>

<style>
  dialog {
    width: 520px;
    max-width: calc(100vw - 32px);
    margin: 96px auto 0;
    padding: var(--space-4);
    color: var(--ink);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  .q {
    width: 100%;
    height: 44px;
    padding: 0 var(--space-3);
    font-size: 16px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .q:focus-visible {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }
  ul {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    max-height: 360px;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 36px;
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    cursor: pointer;
    color: var(--ink-muted);
  }
  li.on {
    background: var(--selected);
    color: var(--ink);
  }
  .label {
    flex: 1;
    color: var(--ink);
    font-weight: 500;
  }
  .none {
    cursor: default;
  }
  .tip {
    margin: var(--space-2) 0 0;
  }
</style>
