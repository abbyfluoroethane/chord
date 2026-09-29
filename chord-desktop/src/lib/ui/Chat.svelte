<script lang="ts">
  import ChatHeader from './ChatHeader.svelte';
  import Composer from './Composer.svelte';
  import MessageList from './MessageList.svelte';
  import { app } from './app.svelte';
</script>

<section class="chat" aria-label={app.channel ? app.channel.name : 'Chat'}>
  <ChatHeader />
  {#if app.channel}
    <MessageList />
    <Composer />
  {:else}
    <div class="empty">
      <p>Pick a channel to start.</p>
    </div>
  {/if}
</section>

<style>
  .chat {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    background: var(--surface-100);
  }
  .empty {
    flex: 1;
    display: grid;
    place-items: center;
    color: var(--ink-muted);
  }
</style>
