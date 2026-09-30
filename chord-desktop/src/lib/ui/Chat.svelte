<script lang="ts">
  import ChatHeader from './ChatHeader.svelte';
  import Composer from './Composer.svelte';
  import MessageList from './MessageList.svelte';
  import { app } from './app.svelte';
</script>

<section
  class="chat"
  class:dropping={app.dropping && !!app.channel}
  aria-label={app.channel ? app.channel.name : 'Chat'}
>
  <ChatHeader />
  {#if app.channel}
    <MessageList />
    <Composer />
    {#if app.dropping}
      <div class="drop-hint" aria-hidden="true">Drop files to send them</div>
    {/if}
  {:else}
    <div class="empty">
      <p>Pick a channel to start.</p>
    </div>
  {/if}
</section>

<style>
  .chat {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    background: var(--surface-100);
  }
  /* A file is over the window: the message list and the composer are the drop target. */
  .drop-hint {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
    background: color-mix(in srgb, var(--surface-100) 82%, transparent);
    border: 2px dashed var(--line);
    color: var(--ink-muted);
    font-weight: 600;
  }
  .empty {
    flex: 1;
    display: grid;
    place-items: center;
    color: var(--ink-muted);
  }
</style>
