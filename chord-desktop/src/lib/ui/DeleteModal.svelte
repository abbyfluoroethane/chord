<script lang="ts">
  import Avatar from './Avatar.svelte';
  import MessageBody from './MessageBody.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { stamp } from './format';
  import type { TimelineItem } from './types';

  let { item, onclose }: { item: TimelineItem; onclose: () => void } = $props();
</script>

<Modal title="Delete message" {onclose}>
  <p class="ask">Delete this message? Nobody can bring it back.</p>
  <div class="preview">
    <Avatar name={item.senderName} src={item.avatar} size={40} cut="var(--surface-100)" />
    <div class="text">
      <span class="head"><b>{item.senderName}</b> <span class="meta">{stamp(item.timestamp)}</span></span>
      <MessageBody body={item.body} />
    </div>
  </div>
  <p class="tip meta">Tip: hold Shift when you click Delete to skip this question.</p>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button
      class="btn btn-danger"
      onclick={() => {
        app.deleteMessage(item);
        onclose();
      }}>Delete</button
    >
  {/snippet}
</Modal>

<style>
  .ask {
    margin: 0 0 var(--space-4);
  }
  .preview {
    display: flex;
    gap: var(--space-3);
    padding: var(--space-3);
    background: var(--surface-100);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    max-height: 200px;
    overflow: hidden;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .tip {
    margin: var(--space-3) 0 0;
  }
</style>
