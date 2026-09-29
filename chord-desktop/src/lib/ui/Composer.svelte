<script lang="ts">
  // Composer: upload, autosizing textarea (144px max), reply banner, typing line.
  import { untrack } from 'svelte';
  import Paperclip from 'lucide-svelte/icons/paperclip';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { live } from './bridge';
  import { ui } from './ui.svelte';
  import { typingText } from './format';
  import { tooltip } from './tooltip';

  let box = $state<HTMLTextAreaElement>();
  let files = $state<HTMLInputElement>();
  // Drafts survive a switch between channels.
  const drafts: Record<string, string> = {};
  let value = $state('');
  let draftFor = '';

  $effect(() => {
    const jid = app.selectedJid;
    untrack(() => {
      if (draftFor) drafts[draftFor] = value;
      draftFor = jid;
      value = drafts[jid] ?? '';
    });
    queueMicrotask(() => {
      fit();
      box?.focus();
    });
  });

  // Focus comes back when a reply starts or an edit ends.
  $effect(() => {
    if (app.replyingTo || app.editingId === null) box?.focus();
  });

  function fit() {
    if (!box) return;
    box.style.height = 'auto';
    box.style.height = `${Math.min(box.scrollHeight, 144)}px`;
  }

  async function send() {
    const text = value;
    if (!text.trim()) return;
    value = '';
    queueMicrotask(fit);
    const ok = await app.send(text);
    // A message that did not go stays in the box, unless the user typed something new.
    if (!ok && !value) value = text;
  }

  function input() {
    fit();
    app.noteTyping(value.trim().length > 0);
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    } else if (e.key === 'Escape' && app.replyingTo) {
      e.preventDefault();
      app.replyingTo = null;
    } else if (e.key === 'ArrowUp' && !value) {
      const last = app.lastOwn();
      if (last) {
        e.preventDefault();
        app.editingId = last.id;
      }
    }
  }

  // The bridge uploads from a file path. Inside the app the system file dialog gives one
  // (a file dropped on the window works too). In a browser, the file input stays.
  async function upload() {
    if (!live) {
      files?.click();
      return;
    }
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ multiple: true, directory: false, title: 'Send a file' });
    const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked];
    for (const path of paths) void app.uploadPath(path);
  }

  function picked() {
    for (const f of files?.files ?? []) app.sendFile(f);
    if (files) files.value = '';
  }

  const placeholder = $derived(
    app.channel ? `Message ${app.channel.kind === 'dm' ? '' : '#'}${app.channel.name}` : 'Message'
  );
  const typing = $derived(typingText(app.typingHere));
</script>

<div class="composer">
  {#if app.replyingTo}
    <div class="reply">
      <span>Replying to <b>{app.replyingTo.senderName}</b></span>
      <button aria-label="Cancel reply" onclick={() => (app.replyingTo = null)}>
        <Icon icon={X} size={16} />
      </button>
    </div>
  {/if}
  <div class="box" class:has-reply={!!app.replyingTo}>
    <button
      class="upload"
      aria-label="Upload a file"
      use:tooltip={{ text: 'Upload a file', side: 'top' }}
      onclick={upload}
    >
      <Icon icon={Paperclip} size={20} />
    </button>
    <input bind:this={files} type="file" multiple hidden onchange={picked} tabindex="-1" />
    <textarea
      bind:this={box}
      bind:value
      rows="1"
      aria-label={placeholder}
      {placeholder}
      oninput={input}
      onkeydown={keydown}
    ></textarea>
  </div>
  <div class="typing" aria-live="polite">
    {#if typing}
      <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
      <span>{typing}</span>
    {/if}
  </div>
</div>

<style>
  .composer {
    flex: none;
    padding: 0 var(--space-4);
  }
  .reply {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 32px;
    padding: 0 var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-bottom: 0;
    border-radius: var(--radius-md) var(--radius-md) 0 0;
    color: var(--ink-muted);
    font-size: 13px;
  }
  .reply b {
    color: var(--ink);
  }
  .reply button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
  }
  .reply button:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .box {
    display: flex;
    align-items: flex-start;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    transition: border-color var(--dur-fast);
  }
  .box.has-reply {
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }
  .box:hover {
    border-color: var(--ink-muted);
  }
  .box:focus-within {
    border-color: var(--accent);
  }
  .upload {
    display: grid;
    place-items: center;
    flex: none;
    width: 44px;
    height: 44px;
    color: var(--ink-muted);
    border-right: 1px solid var(--line);
    margin: 0 var(--space-1) 0 0;
    transition: color var(--dur-fast);
  }
  .upload:hover {
    color: var(--ink);
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 144px;
    padding: 10px var(--space-3);
    resize: none;
    background: none;
    border: 0;
    outline: 0;
    line-height: 22px;
  }
  textarea::placeholder {
    color: var(--ink-muted);
  }
  .typing {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 24px;
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }
  .dots {
    display: inline-flex;
    gap: 3px;
  }
  .dots i {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--ink-muted);
    animation: blink 1.2s infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 0.2s;
  }
  .dots i:nth-child(3) {
    animation-delay: 0.4s;
  }
  @keyframes blink {
    0%,
    60%,
    100% {
      opacity: 0.3;
    }
    30% {
      opacity: 1;
    }
  }
</style>
