<script lang="ts">
  // One link preview: site name, title link, description, and an image. A direct image
  // link shows only the image, and a click opens the lightbox. The image is never loaded
  // from its own address: the bridge downloads it with the filter for private networks and
  // gives a `data:` URL (BRIDGESECURITY-04).
  import type { LinkPreview } from '$lib/chord/types';
  import { openLightbox, preloadLightbox } from './lightbox';
  import { linkPreviews } from './linkpreviews.svelte';

  let { preview }: { preview: LinkPreview } = $props();

  /** True after the image failed to load. */
  let broken = $state(false);

  $effect(() => {
    if (preview.image) linkPreviews.requestImage(preview.image);
  });
  const image = $derived(broken || !preview.image ? null : (linkPreviews.image(preview.image) ?? null));
  const imageOnly = $derived(!preview.title && !preview.description);
  const large = $derived(
    !!image && !preview.description && (!preview.imageWidth || preview.imageWidth >= 400)
  );
  const thumb = $derived(!!image && !large);
  const label = $derived(preview.title ?? preview.siteName ?? preview.url);

  function view() {
    if (image) void openLightbox([{ kind: 'image', src: image, alt: label }], 0);
  }
</script>

{#if imageOnly}
  {#if image}
    <button
      class="only"
      onclick={view}
      onpointerenter={preloadLightbox}
      onfocus={preloadLightbox}
      aria-label="View image"
    >
      <img src={image} alt="" loading="lazy" decoding="async" onerror={() => (broken = true)} />
    </button>
  {/if}
{:else}
  <div class="card">
    <div class="text">
      {#if preview.siteName}<span class="site">{preview.siteName}</span>{/if}
      {#if preview.title}
        <a class="title" href={preview.url} target="_blank" rel="noopener noreferrer"
          >{preview.title}</a
        >
      {/if}
      {#if preview.description}<p class="desc">{preview.description}</p>{/if}
      {#if large && image}
        <img class="large" src={image} alt="" loading="lazy" decoding="async" onerror={() => (broken = true)} />
      {/if}
    </div>
    {#if thumb && image}
      <img class="small" src={image} alt="" loading="lazy" decoding="async" onerror={() => (broken = true)} />
    {/if}
  </div>
{/if}

<style>
  .card {
    display: flex;
    gap: var(--space-3);
    max-width: 440px;
    margin-top: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-left: 3px solid var(--accent);
    border-radius: var(--radius-md);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .site {
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
  }
  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .desc {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    margin: 0;
    color: var(--ink);
    font-size: 14px;
    line-height: 20px;
  }
  .large {
    display: block;
    max-width: 400px;
    max-height: 300px;
    width: 100%;
    margin-top: var(--space-2);
    object-fit: cover;
    border-radius: var(--radius-md);
  }
  .small {
    flex: none;
    width: 80px;
    height: 80px;
    object-fit: cover;
    border-radius: var(--radius-md);
  }
  .only {
    display: block;
    max-width: 400px;
    max-height: 300px;
    margin-top: var(--space-2);
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--surface-200);
    cursor: zoom-in;
  }
  .only img {
    display: block;
    max-width: 100%;
    max-height: 300px;
    object-fit: cover;
  }
</style>
