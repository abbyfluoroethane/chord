<script lang="ts">
  // Top banner when an update is there. It installs only on the user's click. "View" opens
  // Settings → About, which has the notes and the channel.
  import Download from 'lucide-svelte/icons/download';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import { ui } from './ui.svelte';
  import { updates } from './updates.svelte';
  import { bannerText, percentOf } from './updatesdata';

  const info = $derived(updates.banner);
  const s = $derived(updates.status);
</script>

{#if info}
  <div class="banner" role="status">
    <Icon icon={Download} size={16} />
    {#if s.kind === 'downloading'}
      <span>{bannerText(info, 'downloading', percentOf(s.downloaded, s.total))}</span>
    {:else if s.kind === 'ready'}
      <span>{bannerText(info, 'ready', null)}</span>
      <button class="act" onclick={() => void updates.restart()}>Restart to update</button>
    {:else}
      <span>{bannerText(info, 'available', null)}</span>
      <button class="act" onclick={() => void updates.restart()}>Restart to update</button>
    {/if}
    <button class="act" onclick={() => ui.openSettings('about')}>View</button>
    <button class="close" aria-label="Close" onclick={() => updates.dismiss()}>
      <Icon icon={X} size={14} />
    </button>
  </div>
{/if}

<style>
  .banner {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: 32px;
    flex: none;
    padding: 0 40px;
    background: var(--brand);
    color: var(--on-brand);
    font-size: 13px;
    font-weight: 500;
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .act {
    text-decoration: underline;
    font-weight: 600;
  }
  .close {
    position: absolute;
    right: var(--space-2);
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
  }
  .close:hover {
    background: color-mix(in srgb, var(--on-brand) 15%, transparent);
  }
</style>
