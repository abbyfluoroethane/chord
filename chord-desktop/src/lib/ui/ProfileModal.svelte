<script lang="ts">
  // Full profile, 600px. Header, then tabs: Info, Shared spaces, Shared contacts.
  import { onMount } from 'svelte';
  import Ban from 'lucide-svelte/icons/ban';
  import Ellipsis from 'lucide-svelte/icons/ellipsis';
  import UserMinus from 'lucide-svelte/icons/user-minus';
  import UserPlus from 'lucide-svelte/icons/user-plus';
  import Avatar from './Avatar.svelte';
  import EmojiText from './EmojiText.svelte';
  import CircleIcon from './CircleIcon.svelte';
  import CopyAddress from './CopyAddress.svelte';
  import Icon from './Icon.svelte';
  import Menu, { type MenuItem } from './Menu.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { contactsStore } from './contacts.svelte';
  import { dayLabel } from './format';
  import { presenceKind, presenceLabel, spaceKey } from './types';
  import { ui } from './ui.svelte';

  let { address, onclose }: { address: string; onclose: () => void } = $props();

  type Tab = 'info' | 'circles' | 'contacts';
  const tabs: { id: Tab; label: string }[] = [
    { id: 'info', label: 'Info' },
    { id: 'circles', label: 'Shared spaces' },
    { id: 'contacts', label: 'Shared contacts' }
  ];

  const p = $derived(contactsStore.person(address));
  const kind = $derived(presenceKind(p.online, p.show));
  const circles = $derived(contactsStore.sharedCircles(address));
  const mutual = $derived(contactsStore.sharedContacts(address));

  let tab = $state<Tab>('info');
  let more = $state<HTMLElement | null>(null);
  let note = $state('');

  onMount(() => (note = ui.noteFor(address)));

  const menu = $derived<MenuItem[]>([
    p.isContact
      ? { label: 'Remove contact', icon: UserMinus, onselect: () => contactsStore.remove(address) }
      : {
          label: 'Add contact',
          icon: UserPlus,
          disabled: p.isBlocked,
          onselect: () => {
            const r = contactsStore.add(address);
            ui.say(r.ok ? r.message : r.error, !r.ok);
          }
        },
    p.isBlocked
      ? { label: 'Unblock', icon: Ban, danger: true, onselect: () => contactsStore.unblock(address) }
      : { label: 'Block', icon: Ban, danger: true, onselect: () => contactsStore.block(address) }
  ]);

  function tabkey(e: KeyboardEvent) {
    const dir = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!dir) return;
    e.preventDefault();
    const i = tabs.findIndex((t) => t.id === tab);
    tab = tabs[(i + dir + tabs.length) % tabs.length].id;
    queueMicrotask(() => document.getElementById(`ptab-${tab}`)?.focus());
  }

  function message() {
    onclose();
    contactsStore.message(address, p.name);
  }

  function openCircle(key: string) {
    onclose();
    app.selectSpace(key);
  }
</script>

<Modal title="Profile of {p.name}" size="medium" {onclose}>
  {#snippet heading()}
    <div class="head">
      <Avatar name={p.name} src={p.avatar} size={64} presence={kind} cut="var(--surface-200)" />
      <div class="who">
        <h2 class="title">{p.name}</h2>
        <CopyAddress address={p.address} />
      </div>
      {#if !p.isMe}
        <div class="actions">
          <button class="btn btn-primary" onclick={message}>Message</button>
          <button
            class="btn more"
            aria-label="More actions"
            aria-haspopup="menu"
            aria-expanded={more !== null}
            onclick={(e) => (more = more ? null : e.currentTarget)}
          >
            <Icon icon={Ellipsis} size={18} />
          </button>
        </div>
      {/if}
    </div>
  {/snippet}

  <div class="tabs" role="tablist" aria-label="Profile" tabindex="-1" onkeydown={tabkey}>
    {#each tabs as t (t.id)}
      <button
        id="ptab-{t.id}"
        role="tab"
        aria-selected={tab === t.id}
        aria-controls="ppanel"
        tabindex={tab === t.id ? 0 : -1}
        class:on={tab === t.id}
        onclick={() => (tab = t.id)}>{t.label}</button
      >
    {/each}
  </div>

  <div id="ppanel" role="tabpanel" aria-labelledby="ptab-{tab}" class="panel">
    {#if tab === 'info'}
      <dl>
        <dt class="field-label">Status</dt>
        <dd>{#if p.status}<EmojiText text={p.status} />{:else}{presenceLabel[kind]}{/if}</dd>
        <dt class="field-label">Address</dt>
        <dd class="mono">{p.address}</dd>
        {#if p.since}
          <dt class="field-label">Member since</dt>
          <dd>{dayLabel(p.since)}</dd>
        {/if}
      </dl>
      <div class="field">
        <label class="field-label" for="pnote">Note</label>
        <textarea
          id="pnote"
          rows="3"
          maxlength="200"
          placeholder="Only on this device"
          bind:value={note}
          oninput={() => ui.setNote(address, note)}
        ></textarea>
        <span class="meta">Only you can see this. It stays on this device.</span>
      </div>
    {:else if tab === 'circles'}
      {#if circles.length}
        <ul>
          {#each circles as c (spaceKey(c))}
            <li>
              <button class="line" onclick={() => openCircle(spaceKey(c))}>
                <span class="cicon"><CircleIcon name={c.name} src={c.avatar} fill /></span>
                <span>{c.name}</span>
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="empty">You share no spaces.</p>
      {/if}
    {:else if mutual.length}
      <ul>
        {#each mutual as c (c.address)}
          <li>
            <button class="line" onclick={() => (ui.profile = c.address)}>
              <Avatar
                name={c.name}
                src={c.avatar}
                size={32}
                presence={presenceKind(c.online, c.show)}
                cut="var(--surface-200)"
              />
              <span>{c.name}</span>
              <span class="mono meta">{c.address}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">You share no contacts.</p>
    {/if}
  </div>

  {#if more}
    <Menu anchor={more} items={menu} placement="bottom-end" label="More actions" onclose={() => (more = null)} />
  {/if}
</Modal>

<style>
  .head {
    display: flex;
    flex: 1;
    align-items: center;
    gap: var(--space-4);
    min-width: 0;
  }
  .who {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .who h2 {
    margin: 0;
    font-size: 22px;
    line-height: 28px;
  }
  .actions {
    display: flex;
    gap: var(--space-2);
  }
  .more {
    padding: 0;
    width: 36px;
  }
  .tabs {
    display: flex;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
    border-bottom: 1px solid var(--line);
  }
  .tabs:focus {
    outline: none;
  }
  .tabs button {
    height: 36px;
    margin-bottom: -1px;
    border-bottom: 2px solid transparent;
    color: var(--ink-muted);
    font-weight: 500;
  }
  .tabs button:hover {
    color: var(--ink);
  }
  .tabs button.on {
    color: var(--ink);
    border-bottom-color: var(--brand);
  }
  .panel {
    min-height: 200px;
  }
  dl {
    margin: 0 0 var(--space-4);
  }
  dd {
    margin: 0 0 var(--space-3);
  }
  textarea {
    resize: none;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-300);
  }
  textarea:focus-visible {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 48px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    text-align: left;
    font-weight: 500;
  }
  .line:hover {
    background: var(--hover);
  }
  .cicon {
    display: block;
    width: 32px;
    height: 32px;
    overflow: hidden;
    border-radius: var(--radius-circle-icon);
    font-size: 13px;
    font-weight: 600;
  }
  .empty {
    margin: var(--space-6) 0;
    text-align: center;
    color: var(--ink-muted);
  }
</style>
