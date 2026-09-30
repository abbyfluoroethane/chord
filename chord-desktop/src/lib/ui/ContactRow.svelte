<script lang="ts">
  // One row on the contacts page, 62px. Round actions show on hover and on focus.
  import Check from 'lucide-svelte/icons/check';
  import Ellipsis from 'lucide-svelte/icons/ellipsis';
  import MessageSquare from 'lucide-svelte/icons/message-square';
  import UserPlus from 'lucide-svelte/icons/user-plus';
  import ShieldOff from 'lucide-svelte/icons/shield-off';
  import X from 'lucide-svelte/icons/x';
  import Avatar from './Avatar.svelte';
  import RoundButton from './RoundButton.svelte';
  import { contactsStore } from './contacts.svelte';
  import { idleLabel } from './idle';
  import { presenceKind, presenceLabel, type ContactItem } from './types';
  import { ui } from './ui.svelte';

  let {
    item,
    kind
  }: { item: ContactItem; kind: 'contact' | 'incoming' | 'outgoing' | 'blocked' } = $props();

  const presence = $derived(presenceKind(item.online, item.show));
  const playing = $derived(item.activity ? `Listening to ${item.activity}` : null);
  const line = $derived(
    kind === 'incoming'
      ? 'Incoming request'
      : kind === 'outgoing'
        ? 'Outgoing request'
        : kind === 'blocked'
          ? 'Blocked'
          : [item.status ?? presenceLabel[presence], idleLabel(item.idleSince ?? null), playing]
              .filter(Boolean)
              .join(' · ')
  );
  const more = $derived(ui.personMenu?.address === item.address);

  // A click on the row opens the DM. The buttons inside cover the keyboard.
  function open(e: MouseEvent) {
    if ((e.target as HTMLElement).closest('button')) return;
    if (kind === 'contact') contactsStore.message(item.address, item.name);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="row"
  class:more
  class:contact={kind === 'contact'}
  onclick={open}
  oncontextmenu={(e) => ui.openPersonMenu(e, item.address, item.name)}
>
  <Avatar
    name={item.name}
    src={item.avatar}
    size={32}
    presence={kind === 'contact' ? presence : null}
    cut="var(--surface-100)"
  />
  <div class="text">
    <div class="top">
      <button
        class="name"
        aria-haspopup="dialog"
        onclick={(e) => ui.openPopout(item.address, e.currentTarget, item.name, 'right-start')}
        >{item.name}</button
      >
      <span class="addr mono">{item.address}</span>
    </div>
    <span class="meta line">{line}</span>
  </div>

  <div class="actions">
    {#if kind === 'contact'}
      <RoundButton
        icon={MessageSquare}
        label="Message"
        onclick={() => contactsStore.message(item.address, item.name)}
      />
      <RoundButton
        icon={Ellipsis}
        label="More"
        expanded={more}
        onclick={(e) => ui.openPersonMenu(e, item.address, item.name)}
      />
    {:else if kind === 'incoming'}
      <RoundButton icon={Check} label="Accept" tone="good" onclick={() => contactsStore.accept(item.address)} />
      <RoundButton
        icon={UserPlus}
        label="Accept and add back"
        tone="good"
        onclick={() => contactsStore.accept(item.address, true)}
      />
      <RoundButton icon={X} label="Ignore" tone="danger" onclick={() => contactsStore.ignore(item.address)} />
    {:else if kind === 'outgoing'}
      <RoundButton icon={X} label="Cancel" tone="danger" onclick={() => contactsStore.cancel(item.address)} />
    {:else}
      <RoundButton icon={ShieldOff} label="Unblock" onclick={() => contactsStore.unblock(item.address)} />
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: 62px;
    margin: 0 var(--space-6) 0 var(--space-6);
    padding: 0 var(--space-2);
    border-top: 1px solid var(--line);
    border-radius: var(--radius-md);
    transition: background var(--dur-fast);
  }
  .row.contact {
    cursor: pointer;
  }
  .row:hover,
  .row:focus-within {
    background: var(--hover);
    border-top-color: transparent;
  }
  .text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .top {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
  }
  .name:hover {
    text-decoration: underline;
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--ink-muted);
  }
  .line {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    opacity: 0;
    transition: opacity var(--dur-fast);
  }
  .row:hover .actions,
  .row:focus-within .actions,
  .row.more .actions {
    opacity: 1;
  }
</style>
