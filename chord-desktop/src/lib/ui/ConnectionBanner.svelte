<script lang="ts">
  // Top banner while the connection is down. Plain words, no jokes.
  import WifiOff from 'lucide-svelte/icons/wifi-off';
  import Icon from './Icon.svelte';
  import { session } from './session.svelte';
</script>

{#if session.state === 'reconnecting'}
  <div class="banner" role="status">
    <Icon icon={WifiOff} size={16} />
    <span>Can't reach {session.host}. Your messages will send when it's back.</span>
    {#if session.canRetry}
      <button class="retry" onclick={() => void session.retry()}>Try now</button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: 32px;
    flex: none;
    background: var(--brand);
    color: var(--on-brand);
    font-size: 13px;
    font-weight: 500;
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .retry {
    text-decoration: underline;
    font-weight: 600;
  }
</style>
