<script lang="ts">
  // Room settings for an owner or an admin. Maps to api.configureRoom(room, settings).
  // The bridge cannot read the current settings, so "Keep" leaves a value as it is.
  // "All options" loads the whole owner form of the room (api.roomConfigForm) and shows it
  // with the generic form renderer. It saves with api.submitRoomConfigForm.
  import { onMount } from 'svelte';
  import type { DataForm } from '$lib/chord/types';
  import { sampleRoomForm } from '$lib/fixtures/forms';
  import DataFormView from './DataFormView.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { plainError } from './adapt';
  import { problems, submission } from './forms';
  import type { ChannelItem } from './types';
  import { ui } from './ui.svelte';

  let { channel, onclose }: { channel: ChannelItem; onclose: () => void } = $props();

  let name = $state('');
  onMount(() => (name = channel.name));
  let visible = $state<'keep' | 'yes' | 'no'>('keep');
  let membersOnly = $state<'keep' | 'yes' | 'no'>('keep');
  let busy = $state(false);
  /** The whole owner form, once loaded. With it, the modal shows it instead of three fields. */
  let full = $state<DataForm | null>(null);
  let loadError = $state('');
  let showProblems = $state(false);

  async function loadAll() {
    busy = true;
    loadError = '';
    try {
      full = live ? await (await api()).roomConfigForm(channel.jid) : sampleRoomForm();
    } catch (e) {
      loadError = plainError(e);
    } finally {
      busy = false;
    }
  }

  async function saveAll() {
    if (!full) return;
    const found = problems(full);
    if (found.length > 0) {
      showProblems = true;
      loadError = found[0];
      return;
    }
    busy = true;
    loadError = '';
    try {
      if (live) await (await api()).submitRoomConfigForm(channel.jid, submission(full));
      ui.say('Channel settings saved.');
      onclose();
    } catch (e) {
      loadError = plainError(e);
    } finally {
      busy = false;
    }
  }

  const flag = (v: 'keep' | 'yes' | 'no') => (v === 'keep' ? null : v === 'yes');

  async function save() {
    busy = true;
    const r = await app.call((b) =>
      b.configureRoom(channel.jid, {
        name: name.trim() && name.trim() !== channel.name ? name.trim() : null,
        public: flag(visible),
        membersOnly: flag(membersOnly)
      })
    );
    busy = false;
    if (r.ok) onclose();
  }
</script>

<Modal title="Channel settings" size={full ? 'medium' : 'small'} {onclose}>
  {#if full}
    <DataFormView bind:form={full} idPrefix="room" disabled={busy} {showProblems} />
    {#if loadError}<p class="err" role="alert">{loadError}</p>{/if}
  {:else}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="field">
      <label for="room-name">Channel name</label>
      <input id="room-name" class="input" maxlength="60" bind:value={name} autocomplete="off" />
    </div>
    <div class="field">
      <label for="room-public">Shown in the public list</label>
      <select id="room-public" class="input" bind:value={visible}>
        <option value="keep">Keep as it is</option>
        <option value="yes">Yes</option>
        <option value="no">No</option>
      </select>
    </div>
    <div class="field">
      <label for="room-members">Only members can join</label>
      <select id="room-members" class="input" bind:value={membersOnly}>
        <option value="keep">Keep as it is</option>
        <option value="yes">Yes</option>
        <option value="no">No</option>
      </select>
    </div>
    <div class="more">
      <button type="button" class="btn btn-ghost" disabled={busy} onclick={() => void loadAll()}>
        All options…
      </button>
      {#if loadError}<span class="err" role="alert">{loadError}</span>{/if}
    </div>
  </form>
  {/if}

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    {#if full}
      <button class="btn btn-primary" disabled={busy} onclick={() => void saveAll()}>Save</button>
    {:else}
      <button class="btn btn-primary" disabled={busy} onclick={() => void save()}>Save</button>
    {/if}
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .more {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .err {
    margin: var(--space-3) 0 0;
    color: var(--danger);
    font-size: 14px;
  }
</style>
