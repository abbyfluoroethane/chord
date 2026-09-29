<script lang="ts">
  // Profile card, 300px. Opens from member rows, message avatars and names, and contact rows.
  import { onMount } from 'svelte';
  import Avatar from './Avatar.svelte';
  import CopyAddress from './CopyAddress.svelte';
  import Popover from './Popover.svelte';
  import { app } from './app.svelte';
  import { contactsStore } from './contacts.svelte';
  import { tint } from './format';
  import { affiliationLabel, presenceKind, presenceLabel } from './types';
  import { ui, type PopoutState } from './ui.svelte';

  let { state: s }: { state: PopoutState } = $props();

  const p = $derived(contactsStore.person(s.address, s.name));
  const kind = $derived(presenceKind(p.online, p.show));
  const chip = $derived(
    p.affiliation ? (p.role ? `${affiliationLabel(p.affiliation)} · ${p.role}` : affiliationLabel(p.affiliation)) : null
  );

  let note = $state('');
  let noteEl = $state<HTMLTextAreaElement>();
  let text = $state('');

  onMount(() => {
    note = ui.noteFor(s.address);
    // The popover puts focus on its first control. Move it to the note after that.
    if (s.focusNote) setTimeout(() => noteEl?.focus(), 0);
  });

  function close() {
    ui.popout = null;
  }

  function send(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const body = text.trim();
    close();
    contactsStore.message(p.address, p.name);
    if (body) app.send(body);
  }
</script>

<Popover anchor={s.anchor} onclose={close} placement={s.placement} label="Profile of {p.name}">
  <div class="card">
    <div class="banner" style:background={tint(p.name)}></div>
    <div class="avatar">
      <Avatar name={p.name} src={p.avatar} size={80} presence={kind} cut="var(--surface-300)" />
    </div>
    <div class="body">
      <h3 class="title" class:me={p.isMe}>{p.name}</h3>
      <CopyAddress address={p.address} />
      <p class="status">{p.status ?? presenceLabel[kind]}</p>
      {#if chip}<span class="chip">{chip}</span>{/if}

      <hr />
      <div class="field">
        <label class="field-label" for="note-{p.address}">Note</label>
        <textarea
          id="note-{p.address}"
          bind:this={noteEl}
          rows="2"
          maxlength="200"
          placeholder="Only on this device"
          bind:value={note}
          oninput={() => ui.setNote(p.address, note)}
        ></textarea>
      </div>
      <button class="link" onclick={() => ui.openProfile(p.address)}>View profile</button>

      {#if p.isMe}
        <button
          class="btn"
          onclick={() => {
            close();
            ui.openSettings('account');
          }}>Edit profile</button
        >
      {:else}
        <input
          class="input"
          placeholder="Message @{p.name}"
          aria-label="Message {p.name}"
          bind:value={text}
          onkeydown={send}
        />
      {/if}
    </div>
  </div>
</Popover>

<style>
  .card {
    position: relative;
    width: 300px;
  }
  .banner {
    height: 60px;
    border-bottom: 1px solid var(--line);
  }
  .avatar {
    position: absolute;
    top: 20px;
    left: var(--space-4);
    padding: 4px;
    border-radius: 50%;
    background: var(--surface-300);
  }
  .body {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: 52px var(--space-4) var(--space-4);
  }
  h3 {
    margin: 0;
  }
  h3.me {
    color: var(--brand-ink);
  }
  .status {
    margin: 0;
    color: var(--ink-muted);
    font-size: 14px;
  }
  .chip {
    margin-top: var(--space-1);
    padding: 0 var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 20px;
    font-weight: 500;
  }
  hr {
    width: 100%;
    margin: var(--space-2) 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
  .field {
    width: 100%;
  }
  textarea {
    resize: none;
    padding: var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-100);
    font-size: 14px;
    line-height: 20px;
  }
  textarea:focus-visible {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }
  .link {
    color: var(--accent);
    font-size: 14px;
    font-weight: 500;
  }
  .link:hover {
    text-decoration: underline;
  }
  .btn,
  .input {
    width: 100%;
    margin-top: var(--space-2);
  }
</style>
