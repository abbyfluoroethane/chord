<script lang="ts">
  // Room settings for an owner or an admin. Maps to api.configureRoom(room, settings).
  // The bridge cannot read the current settings, so "Keep" leaves a value as it is.
  import { onMount } from 'svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import type { ChannelItem } from './types';

  let { channel, onclose }: { channel: ChannelItem; onclose: () => void } = $props();

  let name = $state('');
  onMount(() => (name = channel.name));
  let visible = $state<'keep' | 'yes' | 'no'>('keep');
  let membersOnly = $state<'keep' | 'yes' | 'no'>('keep');
  let busy = $state(false);

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

<Modal title="Channel settings" {onclose}>
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
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={busy} onclick={() => void save()}>Save</button>
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
</style>
