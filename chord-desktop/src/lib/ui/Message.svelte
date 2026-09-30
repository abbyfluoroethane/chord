<script lang="ts">
  // One message, cozy layout. Grouped follow-ups drop the avatar and name.
  import Avatar from './Avatar.svelte';
  import AttachmentView from './AttachmentView.svelte';
  import DeleteModal from './DeleteModal.svelte';
  import EmojiPicker from './EmojiPicker.svelte';
  import LinkPreviewCard from './LinkPreviewCard.svelte';
  import XmppLinkCard from './XmppLinkCard.svelte';
  import MessageBody from './MessageBody.svelte';
  import MessageEdit from './MessageEdit.svelte';
  import MessageToolbar from './MessageToolbar.svelte';
  import ReactionPills from './ReactionPills.svelte';
  import ReplyPreview from './ReplyPreview.svelte';
  import { actionText } from './action';
  import { contactsStore } from './contacts.svelte';
  import { mayAutoLoad } from './mediatrust';
  import { app } from './app.svelte';
  import { allowsNativeMenu, contextMenu, isMenuKey } from './contextmenu.svelte';
  import { messageMenu, type MessageTarget } from './menus';
  import { clock, domainOf, stamp } from './format';
  import { linkPreviews } from './linkpreviews.svelte';
  import { prefs } from './prefs.svelte';
  import { previewUrls, xmppUrls } from './markdown';
  import type { TimelineItem } from './types';
  import { ui } from './ui.svelte';

  let {
    item,
    grouped,
    onjump
  }: { item: TimelineItem; grouped: boolean; onjump: (id: string) => void } = $props();

  let picker = $state<HTMLElement | null>(null);
  let deleting = $state(false);

  const compact = $derived(prefs.display === 'compact');
  const editing = $derived(app.editingId === item.id);
  const foreign = $derived(domainOf(item.sender) !== domainOf(app.me.address));
  const action = $derived(actionText(item.body));
  const full = $derived(new Date(item.timestamp).toLocaleString());

  // Remote files load on their own only for me and for contacts. For anyone else the user
  // clicks once for this message. Until then the app makes no request to any address that
  // the sender chose (BRIDGESECURITY-03, 04).
  let revealed = $state(false);
  const trusted = $derived(
    revealed ||
      mayAutoLoad(
        item.sender,
        app.me.address,
        (a) => contactsStore.isContact(a),
        linkPreviews.strangers
      )
  );

  // Up to three different links of the body. Code is not a link. The attachment has its own view.
  const candidates = $derived.by(() => {
    if (!linkPreviews.enabled || item.retracted || editing || !item.body) return [];
    const urls = previewUrls(item.body).filter((u) => u !== item.attachment?.url);
    return [...new Set(urls)].slice(0, 3);
  });
  const wanted = $derived(trusted ? candidates : []);

  // Up to three different xmpp: links get a card. The card asks the server that the link
  // names, so the switch for link previews turns the cards off too.
  const cards = $derived.by(() => {
    if (!linkPreviews.enabled || item.retracted || editing || !item.body) return [];
    return [...new Set(xmppUrls(item.body))].slice(0, 3);
  });

  // Ask for the previews when the message first scrolls into view.
  let seen = $state(false);
  function watch(node: HTMLElement) {
    if (typeof IntersectionObserver === 'undefined') {
      seen = true;
      return;
    }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        seen = true;
        io.disconnect();
      }
    });
    io.observe(node);
    return { destroy: () => io.disconnect() };
  }
  $effect(() => {
    if (seen) for (const url of wanted) linkPreviews.request(url);
  });

  // The menu of the message: right-click, the "more" button, or the menu key on a focused
  // message. A right-click on an image or a link adds a first section for it.
  function targetOf(e: Event): MessageTarget {
    if (e.type !== 'contextmenu' || item.retracted) return {};
    const el = e.target as Element | null;
    const a = el?.closest?.('a[href]') as HTMLAnchorElement | null | undefined;
    const image = el?.closest?.('[data-ctx="image"]') ? item.attachment : null;
    const sel = window.getSelection();
    const root = e.currentTarget as Node;
    const text =
      sel && !sel.isCollapsed && sel.anchorNode && root.contains(sel.anchorNode)
        ? sel.toString().trim()
        : '';
    return { link: a?.href ?? null, image, selection: text };
  }

  function openMenu(e: Event) {
    const { items, quick } = messageMenu(item, targetOf(e), (ev) =>
      ev.shiftKey ? app.deleteMessage(item) : (deleting = true)
    );
    contextMenu.open(e, items, { label: 'Message menu', quick });
  }

  function keydown(e: KeyboardEvent) {
    if (editing || !isMenuKey(e) || allowsNativeMenu(e.target)) return;
    openMenu(e);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="msg"
  class:grouped
  class:compact
  class:mention={item.mention && !item.retracted}
  class:editing
  class:sending={item.status === 'sending'}
  id="msg-{item.id}"
  role="article"
  tabindex="-1"
  aria-label="{item.senderName}, {stamp(item.timestamp)}"
  oncontextmenu={openMenu}
  onkeydown={keydown}
>
  {#if !item.retracted && !editing}
    <div class="toolbar">
      <MessageToolbar
        own={app.canEdit(item)}
        onreact={(a) => (picker = a)}
        onreply={() => app.startReply(item)}
        onedit={() => (app.editingId = item.id)}
        onmore={openMenu}
      />
    </div>
  {/if}

  {#if item.replyTo}
    <div class="reply"><ReplyPreview reply={item.replyTo} {onjump} /></div>
  {/if}

  <div class="gutter">
    {#if grouped || compact}
      <time class="hover-time meta" datetime={new Date(item.timestamp).toISOString()} title={full}>
        {clock(item.timestamp)}
      </time>
    {:else}
      <button
        class="who"
        aria-label="Profile of {item.senderName}"
        aria-haspopup="dialog"
        onclick={(e) => ui.openPopout(item.sender, e.currentTarget, item.senderName, 'right-start')}
        oncontextmenu={(e) => ui.openPersonMenu(e, item.sender, item.senderName)}
      >
        <Avatar name={item.senderName} src={item.avatar} size={40} cut="var(--surface-100)" />
      </button>
    {/if}
  </div>

  <div class="main">
    {#if !grouped}
      <div class="head">
        <button
          class="name"
          class:own={item.outgoing}
          class:foreign
          aria-haspopup="dialog"
          onclick={(e) => ui.openPopout(item.sender, e.currentTarget, item.senderName, 'right-start')}
          oncontextmenu={(e) => ui.openPersonMenu(e, item.sender, item.senderName)}
          >{item.senderName}</button
        >
        {#if foreign}<span class="addr mono">@{domainOf(item.sender)}</span>{/if}
        {#if !compact}
          <time class="meta" datetime={new Date(item.timestamp).toISOString()} title={full}>
            {stamp(item.timestamp)}
          </time>
        {/if}
      </div>
    {/if}

    {#if item.retracted}
      <p class="retracted">Message deleted.</p>
    {:else if editing}
      <MessageEdit {item} />
    {:else}
      {#if item.body}
        <div class="text" class:action={action !== null}>
          {#if action !== null}
            <span class="star" aria-hidden="true">*</span>
            <span class="actor">{item.senderName}</span>
            <MessageBody body={action} />
          {:else}
            <MessageBody body={item.body} />
          {/if}
          {#if item.edited}<span class="edited meta">(edited)</span>{/if}
        </div>
      {/if}
      {#if item.attachment}
        <AttachmentView file={item.attachment} {trusted} onload={() => (revealed = true)} />
      {/if}
      {#if cards.length}
        <div class="previews">
          {#each cards as uri (uri)}<XmppLinkCard {uri} {trusted} />{/each}
        </div>
      {/if}
      {#if candidates.length && !trusted}
        <div class="previews">
          <button class="btn" onclick={() => (revealed = true)} title="The site sees your IP address">
            Load link preview
          </button>
        </div>
      {/if}
      {#if wanted.length}
        <div class="previews" use:watch>
          {#each wanted as url (url)}
            {@const p = linkPreviews.get(url)}
            {#if p}<LinkPreviewCard preview={p} />{/if}
          {/each}
        </div>
      {/if}
      <ReactionPills reactions={item.reactions} ontoggle={(e) => app.toggleReaction(item.id, e)} />
      {#if item.status === 'failed'}
        <p class="failed">Not sent. <button onclick={() => app.retry(item)}>Try again</button></p>
      {/if}
    {/if}
  </div>
</div>

{#if picker}
  <EmojiPicker
    anchor={picker}
    onpick={(e) => app.toggleReaction(item.id, e)}
    onclose={() => (picker = null)}
  />
{/if}
{#if deleting}
  <DeleteModal {item} onclose={() => (deleting = false)} />
{/if}

<style>
  .previews {
    display: flex;
    flex-direction: column;
  }
  .msg {
    position: relative;
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr);
    column-gap: var(--space-4);
    padding: 2px var(--space-4) 2px var(--space-4);
    margin-top: var(--space-4);
    transition: background var(--dur-fast);
  }
  .msg:focus {
    outline: none;
  }
  .msg.grouped {
    margin-top: 0;
  }
  .msg:hover,
  .msg:focus-within,
  .msg.editing {
    background: var(--hover);
  }
  .msg.mention {
    background: var(--brand-soft);
  }
  .msg.mention::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--brand);
  }
  .msg.mention:hover {
    background: color-mix(in srgb, var(--brand-soft) 88%, var(--ink));
  }
  .msg.sending {
    opacity: 0.6;
  }
  :global(.flash) {
    animation: flash 1.6s var(--ease-out);
  }
  @keyframes flash {
    0%,
    40% {
      background: color-mix(in srgb, var(--accent) 22%, transparent);
    }
  }

  .toolbar {
    position: absolute;
    right: var(--space-4);
    top: -16px;
    z-index: 2;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--dur-fast);
  }
  .msg:hover .toolbar,
  .msg:focus-within .toolbar {
    opacity: 1;
    pointer-events: auto;
  }

  .reply {
    grid-column: 2;
    grid-row: 1;
    position: relative;
    min-width: 0;
    padding-left: var(--space-3);
  }
  /* The connector from the avatar up to the quoted line */
  .reply::before {
    content: '';
    position: absolute;
    left: calc(-1 * (var(--space-4) + 20px));
    left: -36px;
    top: 50%;
    width: 30px;
    height: calc(50% + 6px);
    border-left: 2px solid var(--line);
    border-top: 2px solid var(--line);
    border-top-left-radius: 8px;
  }
  .gutter {
    grid-column: 1;
    grid-row: 2;
    display: flex;
    justify-content: center;
    padding-top: 2px;
  }
  .main {
    grid-column: 2;
    grid-row: 2;
    min-width: 0;
  }
  .hover-time {
    opacity: 0;
    align-self: center;
    font-size: 11px;
    transition: opacity var(--dur-fast);
  }
  .msg:hover .hover-time {
    opacity: 1;
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-height: 22px;
  }
  .name {
    font-weight: 600;
  }
  .name.own {
    color: var(--brand-ink);
  }
  .name.foreign {
    color: var(--accent);
  }
  .addr {
    margin-left: -6px;
    font-size: 12px;
    line-height: 16px;
    color: var(--accent);
  }
  .text {
    font-size: var(--message-size, 15px);
    line-height: var(--message-line, 22px);
  }
  /* XEP-0245: "* Alice waves". The name reads as part of the sentence. */
  .text.action {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: 0.3em;
    font-style: italic;
    color: var(--ink-muted);
  }
  .text.action .actor {
    font-weight: 600;
  }
  .who {
    display: block;
    height: 40px;
    border-radius: 50%;
  }
  .name:hover {
    text-decoration: underline;
  }

  /* Compact: times on the left, no avatars, tight rows */
  .msg.compact {
    grid-template-columns: 48px minmax(0, 1fr);
    column-gap: var(--space-2);
    margin-top: 0;
    padding-top: 1px;
    padding-bottom: 1px;
  }
  .msg.compact .hover-time {
    opacity: 1;
    align-self: start;
    text-align: right;
    line-height: var(--message-line, 22px);
  }
  .msg.compact .gutter {
    justify-content: flex-end;
    padding-top: 0;
  }
  .msg.compact .main {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: var(--space-2);
  }
  .msg.compact .head {
    min-height: 0;
  }
  .msg.compact .text {
    flex: 1 1 0;
    min-width: 0;
  }
  .msg.compact .main > :global(:not(.head):not(.text)) {
    flex-basis: 100%;
  }
  .msg.compact .reply::before {
    display: none;
  }
  .edited {
    margin-left: var(--space-1);
    font-size: 11px;
  }
  .retracted {
    margin: 0;
    color: var(--ink-muted);
    font-style: italic;
  }
  .failed {
    margin: var(--space-1) 0 0;
    color: var(--danger);
    font-size: 13px;
  }
  .failed button {
    text-decoration: underline;
    color: inherit;
  }
</style>
