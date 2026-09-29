<script lang="ts">
  // Small quote line above a reply. Click to jump to the original message.
  import CornerUpLeft from 'lucide-svelte/icons/corner-up-left';
  import Icon from './Icon.svelte';
  import type { ReplyPreview } from './types';

  let { reply, onjump }: { reply: ReplyPreview; onjump: (id: string) => void } = $props();
</script>

<button class="preview" onclick={() => onjump(reply.id)} aria-label="Replying to {reply.senderName}. Jump to message.">
  <Icon icon={CornerUpLeft} size={14} />
  <span class="who">{reply.senderName}</span>
  <span class="text">{reply.body}</span>
</button>

<style>
  .preview {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    max-width: 100%;
    margin-bottom: 2px;
    color: var(--ink-muted);
    font-size: 13px;
    line-height: 18px;
    text-align: left;
  }
  .preview:hover .text {
    color: var(--ink);
  }
  .who {
    font-weight: 600;
    flex: none;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: color var(--dur-fast);
  }
</style>
