<script lang="ts">
  // The ban list or the member list of a room, with a button to undo each entry.
  // Maps to api.roomAffiliations(room, affiliation) and api.setRoomAffiliation(room, jid, 'none').
  import { onMount } from 'svelte';
  import { sampleAffiliations } from '$lib/fixtures/forms';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { plainError } from './adapt';

  let { room, affiliation }: { room: string; affiliation: 'outcast' | 'member' } = $props();

  let rows = $state<[string, string | null][]>([]);
  let loading = $state(true);
  let error = $state('');
  let busy = $state('');

  const empty = $derived(
    affiliation === 'outcast' ? 'Nobody is banned from this channel.' : 'This channel has no listed members.'
  );
  const undo = $derived(affiliation === 'outcast' ? 'Unban' : 'Remove');

  onMount(async () => {
    try {
      rows = live ? await (await api()).roomAffiliations(room, affiliation) : sampleAffiliations(affiliation);
    } catch (e) {
      error = plainError(e);
    } finally {
      loading = false;
    }
  });

  async function remove(jid: string) {
    busy = jid;
    // The preview has no bridge: it only takes the row out.
    const ok = !live || (await app.call((b) => b.setRoomAffiliation(room, jid, 'none'))).ok;
    busy = '';
    if (ok) rows = rows.filter(([j]) => j !== jid);
  }
</script>

{#if loading}
  <p class="note">Loading…</p>
{:else if error}
  <p class="note err" role="alert">{error}</p>
{:else if rows.length === 0}
  <p class="note">{empty}</p>
{:else}
  <ul>
    {#each rows as [jid, nick] (jid)}
      <li>
        <span class="who">
          {#if nick}<span class="nick">{nick}</span>{/if}
          <span class="mono jid">{jid}</span>
        </span>
        <button class="btn btn-ghost" disabled={busy === jid} onclick={() => void remove(jid)}>
          {undo}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-height: 320px;
    overflow-y: auto;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .nick {
    font-weight: 600;
  }
  .jid {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-muted);
    font-size: 13px;
  }
  .note {
    margin: 0;
    color: var(--ink-muted);
  }
  .err {
    color: var(--danger);
  }
</style>
