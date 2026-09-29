<script lang="ts">
  // An attachment: an image opens the lightbox (PhotoSwipe), audio and video play inline
  // (Media Chrome controls). Anything else, or media the webview cannot play, is a file card.
  import 'media-chrome';
  import Download from 'lucide-svelte/icons/download';
  import File from 'lucide-svelte/icons/file';
  import Icon from './Icon.svelte';
  import { app } from './app.svelte';
  import { fileSize } from './format';
  import { openLightbox } from './lightbox';
  import type { Attachment } from './types';

  let { file }: { file: Attachment } = $props();

  /** True after the webview failed to load or play the media. */
  let broken = $state(false);

  const kind = $derived.by(() => {
    if (broken) return 'file';
    if (file.mime.startsWith('image/')) return 'image';
    if (file.mime.startsWith('video/')) return 'video';
    if (file.mime.startsWith('audio/')) return 'audio';
    return 'file';
  });
  const ratio = $derived(file.width && file.height ? `${file.width} / ${file.height}` : '16 / 10');

  // The lightbox pages through every image of the open channel.
  function view() {
    const images = app.items
      .map((m) => m.attachment)
      .filter((a): a is Attachment => !!a && a.mime.startsWith('image/'))
      .map((a) => ({ src: a.url, alt: a.name }));
    const index = images.findIndex((i) => i.src === file.url);
    void openLightbox(images, index);
  }
</script>

{#if kind === 'image'}
  <button class="image" style:aspect-ratio={ratio} onclick={view} aria-label="View {file.name}">
    <img src={file.url} alt={file.name} loading="lazy" onerror={() => (broken = true)} />
  </button>
{:else if kind === 'video'}
  <media-controller class="player video">
    <!-- A shared file has no caption track. -->
    <!-- svelte-ignore a11y_media_has_caption -->
    <video
      slot="media"
      src={file.url}
      preload="metadata"
      playsinline
      onerror={() => (broken = true)}
    ></video>
    <media-control-bar>
      <media-play-button></media-play-button>
      <media-time-range></media-time-range>
      <media-time-display showduration></media-time-display>
      <media-mute-button></media-mute-button>
      <media-volume-range></media-volume-range>
      <media-fullscreen-button></media-fullscreen-button>
    </media-control-bar>
  </media-controller>
{:else if kind === 'audio'}
  <div class="audio-card">
    <span class="name">{file.name}</span>
    <media-controller audio class="player audio">
      <audio slot="media" src={file.url} preload="metadata" onerror={() => (broken = true)}></audio>
      <media-control-bar>
        <media-play-button></media-play-button>
        <media-time-range></media-time-range>
        <media-time-display showduration></media-time-display>
        <media-mute-button></media-mute-button>
      </media-control-bar>
    </media-controller>
  </div>
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
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--surface-200);
    cursor: zoom-in;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  /* Media Chrome controls in Chord colours. */
  .player {
    --media-primary-color: var(--ink);
    --media-secondary-color: transparent;
    --media-text-color: var(--ink);
    --media-icon-color: var(--ink);
    --media-control-background: transparent;
    --media-control-hover-background: var(--hover);
    --media-range-bar-color: var(--brand);
    --media-range-track-background: var(--line);
    --media-range-thumb-background: var(--brand);
    --media-font-family: var(--font-sans);
    --media-font-size: 12px;
    display: block;
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .player media-control-bar {
    width: 100%;
    background: color-mix(in srgb, var(--surface-100) 80%, transparent);
  }
  .video {
    max-width: 400px;
    margin-top: var(--space-1);
    border: 1px solid var(--line);
    background: #000;
  }
  .video video {
    display: block;
    width: 100%;
    max-height: 300px;
  }
  .audio-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-width: 400px;
    margin-top: var(--space-1);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .audio-card .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
  }
  .audio {
    --media-background-color: transparent;
    width: 100%;
  }
  .audio media-control-bar {
    background: transparent;
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
