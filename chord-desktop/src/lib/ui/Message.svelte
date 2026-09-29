<script lang="ts">
  // One message, cozy layout. Grouped follow-ups drop the avatar and name.
  import Copy from 'lucide-svelte/icons/copy';
  import Trash from 'lucide-svelte/icons/trash-2';
  import Avatar from './Avatar.svelte';
  import AttachmentView from './AttachmentView.svelte';
  import DeleteModal from './DeleteModal.svelte';
  import EmojiPicker from './EmojiPicker.svelte';
  import Menu, { type MenuItem } from './Menu.svelte';
  import MessageBody from './MessageBody.svelte';
  import MessageEdit from './MessageEdit.svelte';
  import MessageToolbar from './MessageToolbar.svelte';
  import ReactionPills from './ReactionPills.svelte';
  import ReplyPreview from './ReplyPreview.svelte';
  import Pencil from 'lucide-svelte/icons/pencil';
  import Reply from 'lucide-svelte/icons/reply';
  import { app } from './app.svelte';
  import { clock, domainOf, stamp } from './format';
  import type { TimelineItem } from './types';

  let {
    item,
    grouped,
    onjump
  }: { item: TimelineItem; grouped: boolean; onjump: (id: string) => void } = $props();

  let picker = $state<HTMLElement | null>(null);
  let more = $state<HTMLElement | null>(null);
  let deleting = $state(false);

  const editing = $derived(app.editingId === item.id);
  const foreign = $derived(domainOf(item.sender) !== domainOf(app.me.address));
  const full = $derived(new Date(item.timestamp).toLocaleString());

  const items = $derived<MenuItem[]>([
    { label: 'Reply', icon: Reply, onselect: () => app.startReply(item) },
    ...(item.outgoing
      ? [{ label: 'Edit message', icon: Pencil, onselect: () => (app.editingId = item.id) }]
      : []),
    { label: 'Copy text', icon: Copy, onselect: () => void navigator.clipboard?.writeText(item.body) },
    ...(item.outgoing
      ? [
          {
            label: 'Delete message',
            icon: Trash,
            danger: true,
            separator: true,
            // Shift-click skips the question.
            onselect: (e: MouseEvent | KeyboardEvent) =>
              e.shiftKey ? app.retract(item.id) : (deleting = true)
          }
        ]
      : [])
  ]);
</script>

<div
  class="msg"
  class:grouped
  class:mention={item.mention && !item.retracted}
  class:editing
  class:sending={item.status === 'sending'}
  id="msg-{item.id}"
  role="article"
  aria-label="{item.senderName}, {stamp(item.timestamp)}"
>
  {#if !item.retracted && !editing}
    <div class="toolbar">
      <MessageToolbar
        own={item.outgoing}
        onreact={(a) => (picker = a)}
        onreply={() => app.startReply(item)}
        onedit={() => (app.editingId = item.id)}
        onmore={(a) => (more = more ? null : a)}
      />
    </div>
  {/if}

  {#if item.replyTo}
    <div class="reply"><ReplyPreview reply={item.replyTo} {onjump} /></div>
  {/if}

  <div class="gutter">
    {#if grouped}
      <time class="hover-time meta" datetime={new Date(item.timestamp).toISOString()} title={full}>
        {clock(item.timestamp)}
      </time>
    {:else}
      <Avatar name={item.senderName} src={item.avatar} size={40} cut="var(--surface-100)" />
    {/if}
  </div>

  <div class="main">
    {#if !grouped}
      <div class="head">
        <span class="name" class:own={item.outgoing} class:foreign>{item.senderName}</span>
        {#if foreign}<span class="addr mono">@{domainOf(item.sender)}</span>{/if}
        <time class="meta" datetime={new Date(item.timestamp).toISOString()} title={full}>
          {stamp(item.timestamp)}
        </time>
      </div>
    {/if}

    {#if item.retracted}
      <p class="retracted">Message deleted.</p>
    {:else if editing}
      <MessageEdit {item} />
    {:else}
      {#if item.body}
        <div class="text">
          <MessageBody body={item.body} />
          {#if item.edited}<span class="edited meta">(edited)</span>{/if}
        </div>
      {/if}
      {#if item.attachment}<AttachmentView file={item.attachment} />{/if}
      <ReactionPills reactions={item.reactions} ontoggle={(e) => app.toggleReaction(item.id, e)} />
      {#if item.status === 'failed'}
        <p class="failed">Not sent. <button onclick={() => (item.status = 'sending')}>Try again</button></p>
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
{#if more}
  <Menu anchor={more} {items} placement="bottom-end" label="Message menu" onclose={() => (more = null)} />
{/if}
{#if deleting}
  <DeleteModal {item} onclose={() => (deleting = false)} />
{/if}

<style>
  .msg {
    position: relative;
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr);
    column-gap: var(--space-4);
    padding: 2px var(--space-4) 2px var(--space-4);
    margin-top: var(--space-4);
    transition: background var(--dur-fast);
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
    line-height: 22px;
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
