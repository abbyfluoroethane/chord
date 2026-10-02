<script lang="ts">
  // Small quote line above a reply. A click on the quote jumps to the message that this
  // one answers, and loads older messages first when it is not in the page. The button
  // after the quote opens the reply chain: the messages that this one answers, and the
  // ones that answer it. A click on a row of the chain jumps to it.
  import CornerUpLeft from 'lucide-svelte/icons/corner-up-left';
  import ListTree from 'lucide-svelte/icons/list-tree';
  import { actionLine } from './action';
  import Icon from './Icon.svelte';
  import Popover from './Popover.svelte';
  import { app } from './app.svelte';
  import { clock } from './format';
  import { replyChain } from './replychain';
  import { tooltip } from './tooltip';
  import type { ReplyPreview, TimelineItem } from './types';

  let {
    reply,
    item,
    onjump
  }: { reply: ReplyPreview; item: TimelineItem; onjump: (id: string) => void } = $props();

  let button = $state<HTMLButtonElement>();
  let open = $state(false);
  const chain = $derived(open ? replyChain(app.items, item) : []);

  function pick(id: string) {
    open = false;
    onjump(id);
  }
</script>

<div class="quote">
  <button
    class="preview"
    onclick={() => onjump(reply.id)}
    aria-label="Replying to {reply.senderName}. Jump to the original message."
    use:tooltip={{ text: 'Jump to the original message', side: 'top' }}
  >
    <Icon icon={CornerUpLeft} size={14} />
    <span class="who">{reply.senderName}</span>
    <span class="text">{actionLine(reply.senderName, reply.body)}</span>
  </button>
  <button
    bind:this={button}
    class="chain"
    class:open
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-label="Show the reply chain"
    use:tooltip={{ text: 'Show the reply chain', side: 'top' }}
    onclick={() => (open = !open)}
  >
    <Icon icon={ListTree} size={14} />
  </button>
</div>

{#if open && button}
  <Popover anchor={button} onclose={() => (open = false)} label="Reply chain">
    <div class="panel">
      <p class="head">Reply chain</p>
      {#if chain.length < 2}
        <p class="note">The message that this one answers is older than the loaded ones.</p>
      {/if}
      <ol>
        {#each chain as m (m.id)}
          <li>
            <button class="row" class:current={m === item} onclick={() => pick(m.id)}>
              <span class="meta">
                <span class="name">{m.senderName}</span>
                <span class="when">{clock(m.timestamp)}</span>
              </span>
              <span class="body">{m.retracted ? 'Message deleted' : actionLine(m.senderName, m.body) || 'Attachment'}</span>
            </button>
          </li>
        {/each}
      </ol>
      <button class="jump" onclick={() => pick(reply.id)}>Jump to the original message</button>
    </div>
  </Popover>
{/if}

<style>
  .quote {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    margin-bottom: 2px;
  }
  .preview {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    color: var(--ink-muted);
    font-size: 13px;
    line-height: 18px;
    text-align: left;
  }
  .preview:hover .text {
    color: var(--ink);
    text-decoration: underline;
  }
  .chain {
    display: inline-flex;
    flex: none;
    padding: 2px;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    opacity: 0;
    transition: opacity var(--dur-fast);
  }
  .quote:hover .chain,
  .chain:focus-visible,
  .chain.open {
    opacity: 1;
  }
  .chain:hover {
    color: var(--ink);
    background: var(--hover);
  }
  .who {
    font-weight: 600;
    flex: none;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: color var(--dur-fast);
  }
  .panel {
    width: 340px;
    max-height: 420px;
    overflow-y: auto;
    padding: var(--space-1);
  }
  .head {
    margin: 0;
    padding: var(--space-2) var(--space-3) var(--space-1);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .note {
    margin: 0;
    padding: var(--space-1) var(--space-3);
    font-size: 13px;
    color: var(--ink-muted);
  }
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row {
    display: flex;
    flex-direction: column;
    width: 100%;
    gap: 2px;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    border-left: 2px solid transparent;
    text-align: left;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.current {
    border-left-color: var(--brand);
    background: var(--brand-soft);
  }
  .meta {
    display: flex;
    gap: var(--space-2);
    font-size: 12px;
    color: var(--ink-muted);
  }
  .name {
    font-weight: 600;
    color: var(--ink);
  }
  .body {
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .jump {
    width: 100%;
    margin-top: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--line);
    font-size: 13px;
    color: var(--brand-ink);
    text-align: left;
  }
  .jump:hover {
    background: var(--hover);
  }
</style>
