<script lang="ts">
  // Scrolling timeline: date dividers, the "new" line, grouping, jump-to-present.
  import { untrack } from 'svelte';
  import ArrowDown from 'lucide-svelte/icons/arrow-down';
  import Message from './Message.svelte';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { live } from './bridge';
  import { clock, dayLabel, sameDay } from './format';
  import { jumpToMessage } from './search';
  import { isGroup, type TimelineItem } from './types';

  type Row =
    | { kind: 'day'; key: string; label: string }
    | { kind: 'new'; key: string }
    | { kind: 'msg'; key: string; item: TimelineItem; grouped: boolean };

  let scroller = $state<HTMLDivElement>();
  let atBottom = $state(true);

  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    let prev: TimelineItem | null = null;
    for (const item of app.items) {
      let broke = false;
      if (!prev || !sameDay(prev.timestamp, item.timestamp)) {
        out.push({ kind: 'day', key: `day-${item.id}`, label: dayLabel(item.timestamp) });
        broke = true;
      }
      if (item.id === app.dividerId) {
        out.push({ kind: 'new', key: `new-${item.id}` });
        broke = true;
      }
      out.push({
        kind: 'msg',
        key: item.id,
        item,
        grouped: !broke && item.sameSenderAsPrevious && !item.replyTo
      });
      prev = item;
    }
    return out;
  });

  const newCount = $derived.by(() => {
    if (!app.dividerId) return 0;
    const i = app.items.findIndex((m) => m.id === app.dividerId);
    return i < 0 ? 0 : app.items.length - i;
  });
  const newSince = $derived(
    app.items.find((m) => m.id === app.dividerId)?.timestamp ?? 0
  );

  function toBottom(smooth = false) {
    if (!scroller) return;
    scroller.scrollTo({ top: scroller.scrollHeight, behavior: smooth ? 'smooth' : 'instant' });
  }

  // Older messages: at the top of the list, ask for more (api.timelinePaginateBack).
  // The new rows arrive above the view. Then the scroll position moves down by their height.
  let loadingOlder = false;
  let keep: { height: number; first: string | undefined } | null = null;

  function onscroll() {
    if (!scroller) return;
    atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
    const scrolls = scroller.scrollHeight > scroller.clientHeight;
    if (live && scrolls && scroller.scrollTop < 60 && !loadingOlder && app.items.length > 0) {
      loadingOlder = true;
      keep = { height: scroller.scrollHeight, first: app.items[0]?.id };
      void app.paginateBack(30).finally(() =>
        setTimeout(() => {
          loadingOlder = false;
          keep = null;
        }, 800)
      );
    }
  }

  // Keep the reader on the same message when older ones arrive above it.
  $effect(() => {
    const first = app.items[0]?.id;
    if (keep && first !== keep.first && scroller) {
      const before = keep.height;
      keep = null;
      queueMicrotask(() => {
        if (scroller) scroller.scrollTop += scroller.scrollHeight - before;
      });
    }
  });

  // The id of the last message that this list followed. Only a new last message moves the
  // view: a scroll, or an update of the last message (a status, a reaction), must not.
  let lastSeen: string | undefined;

  // A new channel opens at the bottom.
  $effect(() => {
    void app.selectedJid;
    lastSeen = undefined;
    queueMicrotask(() => {
      toBottom();
      onscroll();
    });
  });

  // A new message follows along when the reader is at the bottom, or wrote it.
  // atBottom is read untracked: a change of the scroll position must not run this again.
  $effect(() => {
    const last = app.items[app.items.length - 1];
    if (!last || last.id === lastSeen) return;
    const first = lastSeen === undefined;
    lastSeen = last.id;
    const follow = untrack(() => atBottom) || (!first && last.outgoing);
    if (follow) queueMicrotask(() => toBottom(!first));
  });

  function jump(id: string) {
    jumpToMessage(id);
  }
</script>

<div class="wrap">
  {#if app.dividerId}
    <div class="topbar" role="status">
      <span>{newCount} new {newCount === 1 ? 'message' : 'messages'} since {clock(newSince)}</span>
      <button onclick={() => app.markRead()}>Mark as read</button>
    </div>
  {/if}

  <div class="list" bind:this={scroller} {onscroll} role="log" aria-label="Messages" aria-live="polite">
    {#if app.channel}
      <div class="welcome">
        {#if app.channel.kind === 'dm'}
          <h2>{app.channel.name}</h2>
          <p>This is the start of your direct messages with {app.channel.name}.</p>
        {:else if isGroup(app.channel)}
          <h2>{app.channel.name}</h2>
          <p>Welcome to the beginning of the {app.channel.name} group.</p>
        {:else}
          <h2>Welcome to #{app.channel.name}</h2>
          <p>This is the start of the channel.</p>
        {/if}
      </div>
    {/if}
    {#each rows as row (row.key)}
      {#if row.kind === 'day'}
        <div class="divider"><span>{row.label}</span></div>
      {:else if row.kind === 'new'}
        <div class="divider new" role="separator" aria-label="New messages"><span>New</span></div>
      {:else}
        <Message item={row.item} grouped={row.grouped} onjump={jump} />
      {/if}
    {/each}
    <div class="end"></div>
  </div>

  {#if !atBottom}
    <div class="bottombar">
      <span>You are viewing older messages.</span>
      <button onclick={() => toBottom(true)}>
        Jump to present <Icon icon={ArrowDown} size={14} />
      </button>
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overflow-anchor: none;
  }
  .end {
    height: var(--space-4);
  }
  .welcome {
    padding: var(--space-8) var(--space-4) var(--space-2);
  }
  .welcome h2 {
    margin: 0;
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 32px;
    line-height: 36px;
    letter-spacing: -0.03em;
  }
  .welcome p {
    margin: var(--space-2) 0 0;
    color: var(--ink-muted);
  }

  .divider {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-4) var(--space-4) 0;
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }
  .divider::before,
  .divider::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--line);
  }
  .divider.new {
    color: var(--danger);
    text-transform: uppercase;
    font-weight: 600;
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .divider.new::before {
    display: none;
  }
  .divider.new::after {
    background: var(--danger);
  }

  .topbar,
  .bottombar {
    position: absolute;
    left: var(--space-4);
    right: var(--space-4);
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 32px;
    padding: 0 var(--space-3);
    background: var(--surface-300);
    border: 1px solid var(--line);
    font-size: 13px;
    font-weight: 500;
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .topbar {
    top: 0;
    border-top: 0;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }
  .bottombar {
    bottom: var(--space-2);
    border-radius: var(--radius-md);
  }
  .topbar button,
  .bottombar button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--accent);
    font-weight: 600;
  }
  .topbar button:hover,
  .bottombar button:hover {
    text-decoration: underline;
  }
</style>
