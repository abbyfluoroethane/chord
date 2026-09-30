<script lang="ts">
  // Room settings for an owner or an admin. Maps to api.configureRoom(room, settings) and,
  // for the topic, api.setRoomSubject(room, subject).
  // The form of the room gives the current settings (api.roomConfigForm). The two choices
  // start at those values. An admin cannot read that form: there they stay on "Keep as it is".
  // The Banned and Members tabs list the people of the room, each with a button to undo it.
  // "All options" loads the whole owner form of the room (api.roomConfigForm) and shows it
  // with the generic form renderer. It saves with api.submitRoomConfigForm.
  import { onMount } from 'svelte';
  import type { DataForm } from '$lib/chord/types';
  import { sampleRoomForm } from '$lib/fixtures/forms';
  import DataFormView from './DataFormView.svelte';
  import Modal from './Modal.svelte';
  import RoomAffiliationList from './RoomAffiliationList.svelte';
  import Segmented from './Segmented.svelte';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { plainError } from './adapt';
  import { problems, submission } from './forms';
  import { flagChange, flagOf, type Flag } from './roomconfig';
  import type { ChannelItem } from './types';
  import { ui } from './ui.svelte';

  let { channel, onclose }: { channel: ChannelItem; onclose: () => void } = $props();

  let name = $state('');
  let topic = $state('');
  onMount(() => {
    name = channel.name;
    topic = channel.topic ?? '';
  });
  let visible = $state<Flag>('keep');
  let membersOnly = $state<Flag>('keep');
  /** The values that the form showed. A choice that still has one is not sent. */
  let initialVisible: Flag = 'keep';
  let initialMembersOnly: Flag = 'keep';
  type Tab = 'settings' | 'banned' | 'members';
  let tab = $state<Tab>('settings');
  const tabs: { value: Tab; label: string }[] = [
    { value: 'settings', label: 'Settings' },
    { value: 'banned', label: 'Banned' },
    { value: 'members', label: 'Members' }
  ];
  let busy = $state(false);
  /** The whole owner form, once loaded. With it, the modal shows it instead of three fields. */
  let full = $state<DataForm | null>(null);
  let loadError = $state('');
  let showProblems = $state(false);

  /** The form as the server gave it. It stays, so "All options" does not ask again. */
  let loaded: DataForm | null = null;

  async function fetchForm(): Promise<DataForm> {
    return live ? await (await api()).roomConfigForm(channel.jid) : sampleRoomForm();
  }

  // Read the current settings. A failure is not shown: an admin gets no form, and the two
  // choices then keep their "Keep as it is" value.
  onMount(() => {
    void fetchForm()
      .then((form) => {
        loaded = form;
        initialVisible = flagOf(form, 'muc#roomconfig_publicroom');
        initialMembersOnly = flagOf(form, 'muc#roomconfig_membersonly');
        visible = initialVisible;
        membersOnly = initialMembersOnly;
      })
      .catch(() => {
        /* No form for this user. */
      });
  });

  async function loadAll() {
    busy = true;
    loadError = '';
    try {
      full = loaded ?? (await fetchForm());
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

  async function save() {
    busy = true;
    // The topic is a message to the room, not a setting of the form.
    if (topic.trim() !== (channel.topic ?? '') && !(await app.setTopic(channel.jid, topic))) {
      busy = false;
      return;
    }
    const r = await app.call((b) =>
      b.configureRoom(channel.jid, {
        name: name.trim() && name.trim() !== channel.name ? name.trim() : null,
        public: flagChange(initialVisible, visible),
        membersOnly: flagChange(initialMembersOnly, membersOnly)
      })
    );
    busy = false;
    if (r.ok) onclose();
  }
</script>

<Modal title="Channel settings" size={full ? 'medium' : 'small'} {onclose}>
  {#if !full}
    <div class="tabs">
      <Segmented value={tab} options={tabs} label="Channel settings" onchange={(v) => (tab = v)} />
    </div>
  {/if}
  {#if tab === 'banned' && !full}
    <RoomAffiliationList room={channel.jid} affiliation="outcast" />
  {:else if tab === 'members' && !full}
    <RoomAffiliationList room={channel.jid} affiliation="member" />
  {:else if full}
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
      <label for="room-topic">Topic</label>
      <input id="room-topic" class="input" maxlength="300" bind:value={topic} autocomplete="off" />
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
    <button class="btn btn-ghost" onclick={onclose}>{tab === 'settings' || full ? 'Cancel' : 'Close'}</button>
    {#if full}
      <button class="btn btn-primary" disabled={busy} onclick={() => void saveAll()}>Save</button>
    {:else if tab !== 'settings'}
      <!-- The lists change the room at once, so there is nothing to save. -->
    {:else}
      <button class="btn btn-primary" disabled={busy} onclick={() => void save()}>Save</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .tabs {
    margin-bottom: var(--space-4);
  }
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
