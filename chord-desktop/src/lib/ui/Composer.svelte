<script lang="ts">
  // Composer: upload, autosizing textarea (144px max), GIF and emoji picker, reply banner,
  // typing line.
  import { untrack } from 'svelte';
  import Paperclip from 'lucide-svelte/icons/paperclip';
  import Smile from 'lucide-svelte/icons/smile';
  import X from 'lucide-svelte/icons/x';
  import type { Gif } from '$lib/chord';
  import Emoji from './Emoji.svelte';
  import { highlightDraft } from './livemarkdown';
  import ExpressionPicker from './ExpressionPicker.svelte';
  import Icon from './Icon.svelte';
  import { prefs } from './prefs.svelte';
  import { app } from './app.svelte';
  import { live } from './bridge';
  import { ui } from './ui.svelte';
  import { typingText } from './format';
  import { pastedFiles } from './filetransfer';
  import { tooltip } from './tooltip';
  import {
    loadShortcodes,
    replaceShortcodesOutsideCode,
    mayHaveShortcode,
    suggestShortcodes
  } from './shortcodes';

  let box = $state<HTMLTextAreaElement>();
  let boxWrap = $state<HTMLDivElement>();
  let picker = $state<'gif' | 'emoji' | null>(null);
  let pickerTab = $state<'gif' | 'emoji'>('emoji');

  function openPicker(tab: 'gif' | 'emoji') {
    if (picker === tab) {
      picker = null;
      return;
    }
    picker = tab;
    pickerTab = tab;
  }

  function closePicker() {
    picker = null;
    box?.focus();
  }

  /** Put the emoji where the caret is, and keep the caret after it. */
  function insertEmoji(emoji: string) {
    const at = box?.selectionStart ?? value.length;
    const end = box?.selectionEnd ?? at;
    value = value.slice(0, at) + emoji + value.slice(end);
    closePicker();
    queueMicrotask(() => {
      box?.setSelectionRange(at + emoji.length, at + emoji.length);
      fit();
    });
  }

  function sendGif(gif: Gif) {
    closePicker();
    void app.sendGif(gif);
  }
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
    scrollTop = box.scrollTop;
  }

  // The styled copy of the draft under the text box. The box text is transparent, so
  // the user sees the copy, with the real caret and selection on top. The copy keeps
  // every character in the same place: it only changes colours, backgrounds, lines,
  // and a slant or a shadow for italic and bold, never a width.
  const styled = $derived(highlightDraft(value));
  let scrollTop = $state(0);

  async function send() {
    const text = value;
    if (!text.trim()) return;
    value = '';
    codeMatch = null;
    queueMicrotask(fit);
    // Other clients do not know :shortcodes:, so the message goes out with real emoji.
    const out = mayHaveShortcode(text)
      ? replaceShortcodesOutsideCode(text, await loadShortcodes())
      : text;
    const ok = await app.send(out);
    // A message that did not go stays in the box, unless the user typed something new.
    if (!ok && !value) value = text;
  }

  // Shortcode suggestions: after ":" and two letters, a list shows above the box.
  let codeMatch = $state<{ start: number; text: string } | null>(null);
  let suggestions = $state<{ name: string; emoji: string }[]>([]);
  let chosen = $state(0);
  const listOpen = $derived(codeMatch !== null && suggestions.length > 0);

  function scanShortcode() {
    const caret = box?.selectionStart ?? 0;
    const m = /(?:^|\s):([a-z0-9_+-]{2,})$/i.exec(value.slice(0, caret));
    if (!m) {
      codeMatch = null;
      suggestions = [];
      return;
    }
    const text = m[1];
    codeMatch = { start: caret - text.length - 1, text };
    void loadShortcodes().then((map) => {
      if (codeMatch?.text !== text) return;
      suggestions = suggestShortcodes(map, text, 8);
      chosen = 0;
    });
  }

  function pickShortcode(i: number) {
    const pick = suggestions[i];
    if (!pick || !codeMatch) return;
    const at = codeMatch.start;
    const caret = box?.selectionStart ?? at + codeMatch.text.length + 1;
    value = value.slice(0, at) + pick.emoji + value.slice(caret);
    codeMatch = null;
    suggestions = [];
    queueMicrotask(() => {
      box?.setSelectionRange(at + pick.emoji.length, at + pick.emoji.length);
      box?.focus();
      fit();
    });
  }

  function input() {
    fit();
    scanShortcode();
    app.noteTyping(value.trim().length > 0);
  }

  function keydown(e: KeyboardEvent) {
    if (listOpen && !e.isComposing) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const n = suggestions.length;
        chosen = (chosen + (e.key === 'ArrowDown' ? 1 : n - 1)) % n;
        return;
      }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        pickShortcode(chosen);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        codeMatch = null;
        suggestions = [];
        return;
      }
    }
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

  // Inside the app, Rust opens the system file dialog and reads the files: the page never
  // handles a path. In a browser, the file input stays.
  async function upload() {
    if (!live) {
      files?.click();
      return;
    }
    await app.uploadPicked();
  }

  /** A file in the clipboard (a screenshot, for example) goes out as a file. Text pastes as usual. */
  function paste(e: ClipboardEvent) {
    const list = pastedFiles(e.clipboardData);
    if (!list.length) return;
    e.preventDefault();
    if (live) for (const f of list) void app.uploadPasted(f);
    else for (const f of list) app.sendFile(f);
  }

  function picked() {
    for (const f of files?.files ?? []) app.sendFile(f);
    if (files) files.value = '';
  }

  const placeholder = $derived(
    app.channel ? `Message ${app.channel.kind === 'dm' ? '' : '#'}${app.channel.name}` : 'Message'
  );
  const typing = $derived(typingText(app.typingHere));
  // XEP-0245: a body that starts with "/me " shows as an action. It goes out as typed.
  const actionHint = $derived(!typing && /^\/me( |$)/.test(value));
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
  <div class="box" class:has-reply={!!app.replyingTo} bind:this={boxWrap}>
    <button
      class="upload"
      aria-label="Upload a file"
      use:tooltip={{ text: 'Upload a file', side: 'top' }}
      onclick={upload}
    >
      <Icon icon={Paperclip} size={20} />
    </button>
    <input bind:this={files} type="file" multiple hidden onchange={picked} tabindex="-1" />
    <div class="editor">
    <div class="mirror" aria-hidden="true">
      <div class="mirror-text" style:transform="translateY({-scrollTop}px)"
        >{#each styled as r, i (i)}<span class={r.cls}>{r.text}</span>{/each}{#if value.endsWith('\n') || !value}&#8203;{/if}</div
      >
    </div>
    <textarea
      bind:this={box}
      bind:value
      onscroll={() => (scrollTop = box?.scrollTop ?? 0)}
      rows="1"
      aria-label={placeholder}
      {placeholder}
      oninput={input}
      onpaste={paste}
      onkeydown={keydown}
      onclick={scanShortcode}
      onblur={() => (codeMatch = null)}
      role="combobox"
      aria-expanded={listOpen}
      aria-controls="shortcode-list"
      aria-autocomplete="list"
      aria-activedescendant={listOpen ? `shortcode-${chosen}` : undefined}
    ></textarea>
    </div>
    <div class="tools">
      {#if prefs.gifPicker}
        <button
          class="tool"
          class:on={picker === 'gif'}
          aria-label="Send a GIF"
          aria-expanded={picker === 'gif'}
          use:tooltip={{ text: 'Send a GIF', side: 'top' }}
          onclick={() => openPicker('gif')}
        >
          <span class="gif-mark" aria-hidden="true">GIF</span>
        </button>
      {/if}
      <button
        class="tool"
        class:on={picker === 'emoji'}
        aria-label="Add an emoji"
        aria-expanded={picker === 'emoji'}
        use:tooltip={{ text: 'Add an emoji', side: 'top' }}
        onclick={() => openPicker('emoji')}
      >
        <Icon icon={Smile} size={20} />
      </button>
    </div>
    {#if listOpen}
      <ul class="codes" id="shortcode-list" role="listbox" aria-label="Emoji suggestions">
        {#each suggestions as s, i (s.name)}
          <li
            id="shortcode-{i}"
            role="option"
            aria-selected={i === chosen}
            class:on={i === chosen}
            onmousedown={(e) => {
              e.preventDefault();
              pickShortcode(i);
            }}
          >
            <span class="glyph" aria-hidden="true"><Emoji emoji={s.emoji} /></span>
            <span class="name">:{s.name}:</span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  {#if picker && boxWrap}
    <ExpressionPicker
      anchor={boxWrap}
      bind:tab={pickerTab}
      onemoji={insertEmoji}
      ongif={sendGif}
      onclose={closePicker}
    />
  {/if}
  <div class="typing" aria-live="polite">
    {#if typing}
      <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
      <span>{typing}</span>
    {:else if actionHint}
      <span>Sent as an action: * {app.me.name} …</span>
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
  .box {
    position: relative;
  }
  /* The suggestion list: like the other popovers, with a 1px line and no shadow. */
  .codes {
    position: absolute;
    bottom: calc(100% + 4px);
    left: 0;
    z-index: 5;
    min-width: 240px;
    max-width: 100%;
    margin: 0;
    padding: var(--space-1);
    list-style: none;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .codes li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--dur-fast) var(--ease-out);
  }
  .codes li:hover,
  .codes li.on {
    background: var(--hover);
  }
  /* The emoji image is 1.375em: 22px in a 24px box. */
  .codes .glyph {
    flex: none;
    width: 24px;
    font-size: 16px;
    line-height: 24px;
    text-align: center;
  }
  .codes .name {
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }
  .codes li.on .name {
    color: var(--ink);
  }
  .box.has-reply {
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }
  .box:hover {
    border-color: var(--ink-muted);
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
  .editor {
    position: relative;
    flex: 1;
    min-width: 0;
  }
  /* The text box and its styled copy share the font, the size, the padding, and the
     wrap rules, so that each character of the copy lies under the same character. */
  textarea,
  .mirror-text {
    padding: 10px var(--space-3);
    font: inherit;
    line-height: 22px;
    letter-spacing: normal;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    word-break: normal;
    tab-size: 4;
  }
  textarea {
    position: relative;
    display: block;
    width: 100%;
    max-height: 144px;
    resize: none;
    background: none;
    border: 0;
    outline: 0;
    color: transparent;
    -webkit-text-fill-color: transparent;
    caret-color: var(--ink);
    /* No scroll bar: it would make the box narrower than the copy. */
    scrollbar-width: none;
  }
  textarea::-webkit-scrollbar {
    display: none;
  }
  .mirror {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }
  .mirror-text {
    color: var(--ink);
  }
  .mirror :global(.mk) {
    color: var(--ink-muted);
  }
  /* Bold as a thin shadow: the 600 face is wider, and it would move the caret. */
  .mirror :global(.b),
  .mirror :global(.h) {
    text-shadow:
      0.02em 0 0 currentColor,
      -0.02em 0 0 currentColor;
  }
  /* No italic face ships, so italic is a slant of the regular face, at the same widths. */
  .mirror :global(.i) {
    font-style: italic;
  }
  .mirror :global(.u) {
    text-decoration: underline;
  }
  .mirror :global(.s) {
    text-decoration: line-through;
  }
  .mirror :global(.u.s) {
    text-decoration: underline line-through;
  }
  .mirror :global(.code) {
    background: var(--surface-200);
    border-radius: var(--radius-sm);
  }
  .mirror :global(.sp) {
    background: color-mix(in srgb, var(--ink-muted) 30%, transparent);
    border-radius: var(--radius-sm);
  }
  .mirror :global(.link) {
    color: var(--accent);
  }
  .mirror :global(.ts) {
    background: var(--surface-200);
    border-radius: var(--radius-sm);
  }
  .mirror :global(.at) {
    background: var(--brand-soft);
    color: var(--brand-ink);
    border-radius: var(--radius-sm);
  }
  .mirror :global(.sc) {
    color: var(--brand-ink);
  }
  .mirror :global(.sub),
  .mirror :global(.q:not(.mk)) {
    color: var(--ink-muted);
  }
  /* The box text is transparent, so the placeholder sets its own fill. */
  textarea::placeholder {
    color: var(--ink-muted);
    -webkit-text-fill-color: var(--ink-muted);
    opacity: 1;
  }
  .tools {
    display: flex;
    flex: none;
    gap: 2px;
    padding: 6px var(--space-2) 0 0;
  }
  .tool {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .tool:hover,
  .tool.on {
    background: var(--hover);
    color: var(--ink);
  }
  /* The GIF mark: a small outlined label with the 1.5px line of the icons. */
  .gif-mark {
    padding: 1px 3px;
    border: 1.5px solid currentColor;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 600;
    line-height: 12px;
    letter-spacing: 0.02em;
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
