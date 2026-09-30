<script lang="ts">
  // Set the topic (the subject) of a room. Maps to api.setRoomSubject(room, subject).
  // An empty text clears the topic.
  import { onMount } from 'svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { isGroup, type ChannelItem } from './types';

  let { channel, onclose }: { channel: ChannelItem; onclose: () => void } = $props();

  let value = $state('');
  let input = $state<HTMLInputElement>();
  let busy = $state(false);

  onMount(() => {
    value = channel.topic ?? '';
    setTimeout(() => input?.select(), 0);
  });

  async function save() {
    busy = true;
    const ok = await app.setTopic(channel.jid, value);
    busy = false;
    if (ok) onclose();
  }
</script>

<Modal title="Set topic" {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <div class="field">
      <label for="room-topic">Topic of {isGroup(channel) ? '' : '#'}{channel.name}</label>
      <input id="room-topic" class="input" maxlength="300" bind:this={input} bind:value />
      <span class="meta">Everyone in the channel sees it in the header.</span>
    </div>
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={busy} onclick={() => void save()}>Save</button>
  {/snippet}
</Modal>
