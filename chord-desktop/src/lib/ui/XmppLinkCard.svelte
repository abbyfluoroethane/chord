<script lang="ts">
  // The card under a message that holds an xmpp: link, like an invite embed. It asks the
  // server what the space or the room is, and it shows a skeleton while it waits. The
  // button is the only thing that acts, and only on a click. A target that does not exist
  // shows "This invite is not valid", with no button.
  import Hash from 'lucide-svelte/icons/hash';
  import Avatar from './Avatar.svelte';
  import CircleIcon from './CircleIcon.svelte';
  import Icon from './Icon.svelte';
  import { contactsStore } from './contacts.svelte';
  import { presenceKind, presenceLabel } from './types';
  import { xmppLinks } from './xmpplinks.svelte';
  import { parseXmppUri } from './xmppuri';

  let { uri }: { uri: string } = $props();

  const link = $derived(parseXmppUri(uri));
  const info = $derived(link.kind === 'space' || link.kind === 'room' ? xmppLinks.get(link) : undefined);
  const person = $derived(
    link.kind === 'chat' || link.kind === 'contact'
      ? contactsStore.person(link.jid, link.kind === 'contact' ? link.name : null)
      : null
  );
  const joined = $derived(link.kind !== 'unknown' && xmppLinks.joined(link));
  const loading = $derived((link.kind === 'space' || link.kind === 'room') && info === undefined);
  const invalid = $derived(link.kind === 'unknown' || info === null);
  let busy = $state(false);

  $effect(() => {
    if (link.kind === 'space' || link.kind === 'room') xmppLinks.request(link);
  });

  const label = $derived(
    link.kind === 'space' ? 'You were invited to join a space' : link.kind === 'room' ? 'Room' : 'Contact'
  );

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;

  /** The muted line under the name. */
  const detail = $derived.by(() => {
    if (info?.kind === 'space') {
      const parts = [];
      if (info.channels !== null) parts.push(plural(info.channels, 'channel'));
      return { text: info.description, meta: parts.join(' · ') };
    }
    if (info?.kind === 'room') {
      const parts = [];
      if (info.occupants !== null) parts.push(info.occupants === 1 ? '1 person' : `${info.occupants} people`);
      if (info.passwordProtected) parts.push('Password needed');
      return { text: info.subject, meta: parts.join(' · ') };
    }
    if (person) {
      return { text: null, meta: presenceLabel[presenceKind(person.online, person.show)] };
    }
    return { text: null, meta: '' };
  });

  const name = $derived(
    info?.name ?? (person ? person.name : link.kind === 'space' ? link.node : link.kind === 'room' ? link.jid : '')
  );
  const buttonText = $derived(
    link.kind === 'space' || link.kind === 'room'
      ? joined
        ? 'Joined'
        : 'Join'
      : link.kind === 'contact' && person && !person.isContact
        ? 'Add contact'
        : 'Message'
  );

  async function act() {
    if (link.kind === 'unknown' || busy) return;
    busy = true;
    await xmppLinks.act(link, info);
    busy = false;
  }
</script>

<div class="card" role="group" aria-label={label} aria-busy={loading}>
  <span class="label">{label}</span>
  {#if invalid}
    <p class="invalid">This invite is not valid</p>
  {:else if loading}
    <div class="row" aria-hidden="true">
      <span class="tile skeleton"></span>
      <span class="text">
        <span class="bar skeleton" style:width="55%"></span>
        <span class="bar skeleton" style:width="35%"></span>
      </span>
      <span class="btn-skeleton skeleton"></span>
    </div>
    <span class="visually-hidden">Loading</span>
  {:else}
    <div class="row">
      <span class="tile">
        {#if link.kind === 'space'}
          <CircleIcon {name} fill />
        {:else if link.kind === 'room'}
          <Icon icon={Hash} size={24} />
        {:else if person}
          <Avatar name={person.name} src={person.avatar} size={48} presence={person.online ? presenceKind(person.online, person.show) : null} />
        {/if}
      </span>
      <span class="text">
        <span class="name">{link.kind === 'room' ? `#${name}` : name}</span>
        {#if link.kind === 'room' || link.kind === 'chat' || link.kind === 'contact'}
          <span class="meta addr">{link.kind === 'room' ? link.jid : person?.address}</span>
        {/if}
        {#if detail.text}<span class="desc">{detail.text}</span>{/if}
        {#if detail.meta}<span class="meta">{detail.meta}</span>{/if}
      </span>
      <button class="btn btn-primary" disabled={joined || busy} onclick={act}>{buttonText}</button>
    </div>
  {/if}
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 400px;
    margin-top: var(--space-2);
    padding: var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  .label,
  .meta {
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
    font-weight: 500;
    letter-spacing: 0.02em;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: 48px;
    height: 48px;
    border-radius: 12px;
    overflow: hidden;
    color: var(--ink);
    background: var(--surface-300);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .addr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .desc {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 14px;
    line-height: 20px;
  }
  .row .btn {
    flex: none;
  }
  .invalid {
    margin: 0;
    color: var(--ink-muted);
  }
  .skeleton {
    background: var(--surface-300);
    animation: pulse 1.4s ease-in-out infinite;
  }
  .bar {
    display: block;
    height: 12px;
    border-radius: var(--radius-sm);
  }
  .btn-skeleton {
    flex: none;
    width: 64px;
    height: 36px;
    border-radius: var(--radius-md);
  }
  .tile.skeleton {
    border-radius: 12px;
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  @keyframes pulse {
    50% {
      opacity: 0.55;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .skeleton {
      animation: none;
    }
  }
</style>
