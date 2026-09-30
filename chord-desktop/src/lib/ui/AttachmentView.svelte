<script lang="ts">
  // An attachment: a photo opens the viewer (PhotoSwipe). Video and audio play in the chat
  // with the Chord player (Video.js v10), and a video also opens in the viewer. Anything
  // else, or media that the webview cannot play, is a file card.
  import Download from 'lucide-svelte/icons/download';
  import File from 'lucide-svelte/icons/file';
  import Icon from './Icon.svelte';
  import { fileSize } from './format';
  import { viewImage, viewMedia } from './attachments';
  import { preloadLightbox } from './lightbox';
  import MediaPlayer from './media/MediaPlayer.svelte';
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
  let media = $state<HTMLMediaElement>();

  /** Open the video in the viewer where it is now, and continue here after. */
  function expand() {
    const playing = !!media && !media.paused;
    media?.pause();
    viewMedia(file, {
      startAt: media?.currentTime ?? 0,
      playing,
      onclose: (time) => {
        if (media) media.currentTime = time;
      }
    });
  }

  const ratio = $derived(file.width && file.height ? `${file.width} / ${file.height}` : '16 / 10');
</script>

{#if kind === 'image'}
  <button
    class="image"
    data-ctx="image"
    style:aspect-ratio={ratio}
    onclick={() => viewImage(file)}
    onpointerenter={preloadLightbox}
    onfocus={preloadLightbox}
    aria-label="View {file.name}"
  >
    <img
      src={file.url}
      alt={file.name}
      loading="lazy"
      decoding="async"
      onerror={() => (broken = true)}
    />
  </button>
{:else if kind === 'video'}
  <div class="video" style:aspect-ratio={ratio}>
    <MediaPlayer
      kind="video"
      src={file.url}
      name={file.name}
      bind:media
      onexpand={expand}
      onfail={() => (broken = true)}
    />
  </div>
{:else if kind === 'audio'}
  <div class="audio">
    <MediaPlayer kind="audio" src={file.url} name={file.name} onfail={() => (broken = true)} />
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
  /* Every attachment is a card of the style guide: surface-200, a 1px line border and the
     16px corner, at most 400 by 300px. The player cards are in media/media.css. */
  .image {
    display: block;
    max-width: 400px;
    max-height: 300px;
    margin-top: var(--space-1);
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
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

  .video {
    max-width: 400px;
    max-height: 300px;
    margin-top: var(--space-1);
  }
  .audio {
    max-width: 400px;
    margin-top: var(--space-1);
  }

  .card {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    max-width: 400px;
    margin-top: var(--space-1);
    padding: var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
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
