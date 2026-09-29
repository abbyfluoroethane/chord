<script lang="ts">
  // Image preview or file card.
  import Download from 'lucide-svelte/icons/download';
  import File from 'lucide-svelte/icons/file';
  import Icon from './Icon.svelte';
  import { fileSize } from './format';
  import type { Attachment } from './types';

  let { file }: { file: Attachment } = $props();
  const isImage = $derived(file.mime.startsWith('image/'));
  const ratio = $derived(file.width && file.height ? `${file.width} / ${file.height}` : '16 / 10');
</script>

{#if isImage}
  <a class="image" href={file.url} target="_blank" rel="noopener noreferrer" style:aspect-ratio={ratio}>
    <img src={file.url} alt={file.name} loading="lazy" />
  </a>
{:else}
  <div class="card">
    <span class="ico"><Icon icon={File} size={24} /></span>
    <span class="info">
      <a href={file.url} target="_blank" rel="noopener noreferrer">{file.name}</a>
      {#if file.size > 0}<span class="mono size">{fileSize(file.size)}</span>{/if}
    </span>
    <a class="dl" href={file.url} download={file.name} aria-label="Download {file.name}">
      <Icon icon={Download} size={18} />
    </a>
  </div>
{/if}

<style>
  .image {
    display: block;
    max-width: 360px;
    max-height: 260px;
    margin-top: var(--space-1);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .card {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    max-width: 360px;
    margin-top: var(--space-1);
    padding: var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .ico {
    color: var(--accent);
    display: grid;
  }
  .info {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .info a {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size {
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
  }
  .dl {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
  }
  .dl:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
