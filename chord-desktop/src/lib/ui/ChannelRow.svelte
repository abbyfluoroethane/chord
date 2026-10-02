<script lang="ts">
  import Hash from 'lucide-svelte/icons/hash';
  import X from 'lucide-svelte/icons/x';
  import BellOff from 'lucide-svelte/icons/bell-off';
  import Pencil from 'lucide-svelte/icons/pencil';
  import Users from 'lucide-svelte/icons/users';
  import Avatar from './Avatar.svelte';
  import Settings from 'lucide-svelte/icons/settings';
  import ChannelSettings from './ChannelSettings.svelte';
  import TopicModal from './TopicModal.svelte';
  import Icon from './Icon.svelte';
  import { contextMenu } from './contextmenu.svelte';
  import type { MenuItem } from './Menu.svelte';
  import { channelMenu } from './menus';
  import { app } from './app.svelte';
  import { drafts } from './drafts.svelte';
  import { tooltip } from './tooltip';
  import { isGroup, memberLine, presenceKind, type ChannelItem } from './types';
  import { ui } from './ui.svelte';

  let {
    channel,
    selected,
    onclick,
    onclose
  }: {
    channel: ChannelItem;
    selected: boolean;
    onclick: () => void;
    /** DMs: hides the row from the list. Group chats: asks to leave the group. */
    onclose?: () => void;
  } = $props();

  // A chat that is not open and has an unsent draft shows a pencil.
  const hasDraft = $derived(!selected && drafts.has(channel.jid));
  const isDm = $derived(channel.kind === 'dm');
  // A room outside a space shows as a group chat among the DMs, as on Discord.
  const group = $derived(isGroup(channel));
  const sub = $derived(group ? memberLine(channel.members) : '');
  // The bridge sends no presence for a chat that is not open. Then the row shows none.
  const known = $derived(channel.unknownPresence !== true);

  // The menu of a channel. Maps to api.setNotificationLevel(room, level, muteUntil),
  // api.markRead(room), and api.leaveRoom(room).
  let settingsOpen = $state(false);
  let topicOpen = $state(false);
  // Maps to api.setRoomSubject(room, subject). A moderator, or anyone when the room allows it.
  const extra = $derived<MenuItem[]>([
    ...(selected && app.canSetTopic && !isDm && !group
      ? [{ label: 'Set topic', icon: Pencil, onselect: () => (topicOpen = true) }]
      : []),
    ...(selected && app.isRoomAdmin && !isDm
      ? [{ label: 'Channel settings', icon: Settings, onselect: () => (settingsOpen = true) }]
      : [])
  ]);
  const items = $derived<MenuItem[]>(channelMenu(channel, extra));

  function context(e: MouseEvent) {
    contextMenu.open(e, items, { label: `${channel.name} menu` });
  }
  const unread = $derived(channel.unread > 0 && !channel.muted);
  // A DM and a group chat count each unread message. A channel counts the mentions.
  const count = $derived(isDm || group ? channel.unread : channel.mentions);
  const desc = $derived(
    `${isDm || group ? '' : '#'}${channel.name}` +
      (sub ? `, ${sub}` : '') +
      (channel.muted ? ', muted' : '') +
      (hasDraft ? ', draft' : '') +
      (count
        ? `, ${count} ${isDm || group ? 'unread' : count === 1 ? 'mention' : 'mentions'}`
        : unread
          ? ', unread'
          : '')
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="wrap"
  oncontextmenu={isDm && !channel.pm ? (e) => ui.openPersonMenu(e, channel.jid, channel.name) : context}
>
  {#if unread && !selected}<span class="pill"></span>{/if}
  <button
    class="row"
    class:selected
    class:unread
    class:muted={channel.muted}
    class:dm={isDm || group}
    class:offline={isDm && known && !channel.online}
    aria-label={desc}
    aria-current={selected ? 'true' : undefined}
    {onclick}
  >
    {#if isDm}
      <Avatar
        name={channel.name}
        src={channel.avatar}
        size={32}
        presence={known ? presenceKind(channel.online, channel.show) : null}
        cut={selected ? 'color-mix(in srgb, var(--ink) 15%, var(--surface-side))' : 'var(--surface-side)'}
      />
    {:else if group}
      <Avatar name={channel.name} size={32} icon={Users} />
    {:else}
      <span class="hash"><Icon icon={Hash} size={18} /></span>
    {/if}
    {#if sub}
      <span class="text">
        <span class="name">{channel.name}</span>
        <span class="sub">{sub}</span>
      </span>
    {:else}
      <span class="name">{channel.name}</span>
    {/if}
    {#if hasDraft}
      <span class="hash" title="Draft"><Icon icon={Pencil} size={14} /></span>
    {/if}
    {#if channel.muted}
      <span class="hash"><Icon icon={BellOff} size={14} /></span>
    {:else if count > 0}
      <span class="badge" aria-hidden="true">{count > 99 ? '99+' : count}</span>
    {/if}
  </button>
  {#if onclose}
    <button
      class="close"
      aria-label={group ? `Leave ${channel.name}` : `Close DM with ${channel.name}`}
      use:tooltip={{ text: group ? 'Leave group' : 'Close DM', side: 'top' }}
      onclick={onclose}
    >
      <Icon icon={X} size={14} />
    </button>
  {/if}
</div>

{#if settingsOpen}
  <ChannelSettings {channel} onclose={() => (settingsOpen = false)} />
{/if}
{#if topicOpen}
  <TopicModal {channel} onclose={() => (topicOpen = false)} />
{/if}

<style>
  .wrap {
    position: relative;
    padding: 1px var(--space-2);
  }
  .pill {
    position: absolute;
    left: 0;
    top: 50%;
    width: 4px;
    height: 8px;
    transform: translateY(-50%);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    background: var(--ink);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    text-align: left;
    font-weight: 500;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .row.dm {
    height: 42px;
  }
  .row:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .row.selected {
    background: var(--selected);
    color: var(--ink);
    font-weight: 600;
  }
  .row.selected .sub {
    color: var(--ink);
  }
  .row.unread {
    color: var(--ink);
    font-weight: 600;
  }
  .row.muted {
    opacity: 0.55;
  }
  .row.dm.offline .name {
    opacity: 0.7;
  }
  .hash {
    display: grid;
    flex: none;
    color: var(--ink-muted);
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
  }
  .sub {
    overflow: hidden;
    color: var(--ink-muted);
    font-size: 12px;
    font-weight: 400;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    position: absolute;
    right: calc(var(--space-2) + 6px);
    top: 50%;
    display: none;
    place-items: center;
    width: 20px;
    height: 20px;
    transform: translateY(-50%);
    border-radius: var(--radius-sm);
    background: var(--surface-side);
    color: var(--ink-muted);
  }
  .close:hover {
    color: var(--ink);
  }
  .wrap:hover .close,
  .wrap:focus-within .close {
    display: grid;
  }
  .wrap:hover .badge,
  .wrap:focus-within .badge {
    visibility: hidden;
  }
  .badge {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    background: var(--brand);
    color: var(--on-brand);
    font-size: 11px;
    font-weight: 600;
    line-height: 18px;
    text-align: center;
  }
</style>
