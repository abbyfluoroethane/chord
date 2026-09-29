<script lang="ts">
  // Forward a message: pick a channel or a DM from a searchable list. The text goes as a
  // new message, and the address of the file goes with it if the message has one.
  import AtSign from 'lucide-svelte/icons/at-sign';
  import Hash from 'lucide-svelte/icons/hash';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { score } from './fuzzy';
  import { switcherTargets } from './targets';
  import type { TimelineItem } from './types';
  import { ui } from './ui.svelte';

  let { item, onclose }: { item: TimelineItem; onclose: () => void } = $props();

  let query = $state('');
  let index = $state(0);
  let sending = $state(false);

  const places = $derived(switcherTargets().filter((t) => t.kind !== 'circle'));
  const hits = $derived(
    places
      .map((t) => ({ t, s: score(query.trim(), t.label) }))
      .filter((x) => x.s > 0 || !query.trim())
      .sort((a, b) => b.s - a.s)
      .slice(0, 30)
      .map((x) => x.t)
  );

  $effect(() => {
    void hits;
    index = 0;
  });

  const summary = $derived(
    item.body.trim() || (item.attachment ? `File: ${item.attachment.name}` : 'Empty message')
  );

  function focusInput(node: HTMLInputElement) {
    node.focus();
  }

  async function send() {
    const to = hits[index];
    if (!to?.jid || sending) return;
    sending = true;
    const ok = await app.forward(item, to.jid);
    sending = false;
    if (!ok) return;
    ui.say(`Forwarded to ${to.kind === 'channel' ? '#' : ''}${to.label}.`);
    onclose();
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
      void send();
    }
  }
</script>

<Modal title="Forward message" {onclose}>
  <div class="quote">
    <b>{item.senderName}</b>
    <span class="text">{summary}</span>
  </div>
  <input
    class="input"
    role="combobox"
    aria-expanded="true"
    aria-controls="forward-list"
    aria-activedescendant={hits[index] ? `fw-${hits[index].id}` : undefined}
    aria-label="Search channels and people"
    placeholder="Search channels and people"
    autocomplete="off"
    bind:value={query}
    onkeydown={keydown}
    use:focusInput
  />
  <ul id="forward-list" role="listbox" aria-label="Places">
    {#each hits as t, i (t.id)}
      <li
        id="fw-{t.id}"
        role="option"
        aria-selected={i === index}
        class:on={i === index}
        onpointermove={() => (index = i)}
        onclick={() => {
          index = i;
          void send();
        }}
        onkeydown={() => {}}
      >
        <Icon icon={t.kind === 'dm' ? AtSign : Hash} size={16} />
        <span class="label">{t.label}</span>
        <span class="meta">{t.hint}</span>
      </li>
    {:else}
      <li class="none" role="presentation">Nothing matches. Try a shorter name.</li>
    {/each}
  </ul>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={!hits[index] || sending} onclick={() => void send()}>
      Forward
    </button>
  {/snippet}
</Modal>

<style>
  .quote {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-100);
    border: 1px solid var(--line);
    border-left: 3px solid var(--brand);
    border-radius: var(--radius-md);
    font-size: 13px;
  }
  .text {
    color: var(--ink-muted);
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .input {
    width: 100%;
  }
  ul {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    max-height: 280px;
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
    transition: background var(--dur-fast);
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
</style>
