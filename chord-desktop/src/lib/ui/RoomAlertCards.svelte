<script lang="ts">
  // The cards for an invitation to a room, and for a room that was closed. They stack at
  // the top right of the window and stay until the user answers. An invitation has Accept
  // and Decline. A closed room shows the reason, and the room that the owner names instead.
  import Hash from 'lucide-svelte/icons/hash';
  import Icon from './Icon.svelte';
  import { localPart } from './adapt';
  import { contactsStore } from './contacts.svelte';
  import { alertTitle, bareOf, type RoomAlert } from './roomalerts';
  import { roomAlerts } from './roomalerts.svelte';
  import { xmppLinks } from './xmpplinks.svelte';

  let busy = $state<string | null>(null);

  const who = (from: string) => contactsStore.person(bareOf(from)).name;
  const title = (card: RoomAlert) =>
    alertTitle(card, card.kind === 'invite' ? who(card.from) : '');

  /** The name and the detail line of the room of an invitation, from the server. */
  function roomInfo(room: string) {
    const info = xmppLinks.get({ kind: 'room', jid: room, password: null });
    return info?.kind === 'room' ? info : null;
  }

  async function run(card: RoomAlert, act: () => Promise<void>) {
    busy = card.id;
    await act();
    busy = null;
  }
</script>

{#if roomAlerts.list.length}
  <div class="stack" role="region" aria-label="Room invitations and notices">
    {#each roomAlerts.list as card (card.id)}
      {@const info = card.kind === 'invite' ? roomInfo(card.room) : null}
      <div class="card" role="group" aria-label={title(card)}>
        <span class="label">{title(card)}</span>
        <div class="row">
          <span class="tile"><Icon icon={Hash} size={24} /></span>
          <span class="text">
            <span class="name">#{info?.name ?? localPart(card.room)}</span>
            <span class="meta addr">{card.room}</span>
            {#if card.kind === 'invite'}
              {#if card.reason}<span class="desc">{card.reason}</span>{/if}
              {#if info?.passwordProtected || card.password}
                <span class="meta">{card.password ? 'The invitation has the password' : 'Password needed'}</span>
              {/if}
            {:else}
              {#if card.reason}<span class="desc">{card.reason}</span>{/if}
              {#if card.alternate}
                <span class="meta">The owner points to {card.alternate}</span>
              {/if}
            {/if}
          </span>
        </div>
        <div class="actions">
          {#if card.kind === 'invite'}
            <button
              class="btn btn-ghost"
              disabled={busy === card.id}
              onclick={() => run(card, () => roomAlerts.decline(card))}>Decline</button
            >
            <button
              class="btn btn-primary"
              disabled={busy === card.id}
              onclick={() => run(card, () => roomAlerts.accept(card))}>Accept</button
            >
          {:else}
            <button class="btn btn-ghost" onclick={() => roomAlerts.dismiss(card.id)}>Dismiss</button>
            {#if card.alternate}
              <button
                class="btn btn-primary"
                disabled={busy === card.id}
                onclick={() => run(card, () => roomAlerts.goToAlternate(card))}>Go to the new room</button
              >
            {/if}
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .stack {
    position: fixed;
    top: var(--space-4);
    right: var(--space-4);
    z-index: 80;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: 360px;
    max-width: calc(100vw - 2 * var(--space-4));
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-popover, 0 8px 24px rgb(0 0 0 / 0.25));
    animation: arrive var(--dur-arrive) var(--ease-out);
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
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 14px;
    line-height: 20px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
</style>
