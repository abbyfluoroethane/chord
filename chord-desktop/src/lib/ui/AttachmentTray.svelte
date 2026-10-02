<script lang="ts">
  // The attachment tray: the files that wait above the composer. An image or a video shows
  // a thumbnail. Other files show a card with the name and the size. Nothing uploads here:
  // the composer uploads when the user sends.
  import File from 'lucide-svelte/icons/file';
  import RotateCw from 'lucide-svelte/icons/rotate-cw';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import { fileSize } from './format';
  import { previewKind, type TrayItem } from './traydata';

  let {
    items,
    onremove,
    onretry
  }: {
    items: TrayItem[];
    onremove: (id: string) => void;
    onretry: () => void;
  } = $props();
</script>

{#if items.length}
  <ul class="tray" aria-label="Files to send">
    {#each items as item (item.id)}
      {@const kind = item.preview ? previewKind(item.mime) : null}
      <li class="item" class:failed={item.status === 'failed'} class:busy={item.status === 'uploading'}>
        <div class="thumb">
          {#if kind === 'image'}
            <img src={item.preview} alt="" />
          {:else if kind === 'video'}
            <!-- svelte-ignore a11y_media_has_caption -->
            <video src="{item.preview}#t=0.1" preload="metadata" muted playsinline></video>
          {:else}
            <span class="card"><Icon icon={File} size={28} /></span>
          {/if}
          {#if item.status === 'uploading'}
            <span class="shade" role="status"><span class="spin" aria-hidden="true"></span>Uploading</span>
          {:else if item.status === 'failed'}
            <span class="shade bad" role="alert">
              <span class="why" title={item.error}>{item.error}</span>
              <button class="retry" onclick={onretry}><Icon icon={RotateCw} size={14} />Retry</button>
            </span>
          {/if}
        </div>
        <div class="meta">
          <span class="name" title={item.name}>{item.name}</span>
          {#if item.size !== null}<span class="size">{fileSize(item.size)}</span>{/if}
        </div>
        {#if item.status !== 'uploading'}
          <button class="remove" aria-label="Remove {item.name}" onclick={() => onremove(item.id)}>
            <Icon icon={X} size={14} />
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .tray {
    display: flex;
    gap: var(--space-3);
    margin: 0;
    padding: var(--space-3);
    overflow-x: auto;
    list-style: none;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-bottom: 0;
    border-radius: var(--radius-md) var(--radius-md) 0 0;
  }
  .item {
    position: relative;
    flex: none;
    width: 144px;
  }
  .thumb {
    position: relative;
    display: grid;
    place-items: center;
    height: 104px;
    overflow: hidden;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    color: var(--ink-muted);
  }
  .failed .thumb {
    border-color: var(--danger, #e5484d);
  }
  .thumb img,
  .thumb video {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .meta {
    display: flex;
    flex-direction: column;
    margin-top: var(--space-1);
    font-size: 12px;
    line-height: 16px;
  }
  .name {
    overflow: hidden;
    color: var(--ink);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size {
    color: var(--ink-muted);
  }
  .shade {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-2);
    background: color-mix(in srgb, var(--surface-300) 82%, transparent);
    color: var(--ink);
    font-size: 12px;
    text-align: center;
  }
  .why {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .spin {
    width: 18px;
    height: 18px;
    border: 2px solid var(--line);
    border-top-color: var(--ink);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation-duration: 2.4s;
    }
  }
  .retry {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    height: 24px;
    padding: 0 var(--space-2);
    background: var(--hover);
    border-radius: var(--radius-sm);
    color: var(--ink);
    font-size: 12px;
  }
  .retry:hover {
    background: var(--line);
  }
  .remove {
    position: absolute;
    top: -6px;
    right: -6px;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: 50%;
    color: var(--ink-muted);
  }
  .remove:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
