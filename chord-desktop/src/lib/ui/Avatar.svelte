<script lang="ts">
  import { initials, tint } from './format';
  import Presence from './Presence.svelte';
  import type { PresenceKind } from './types';

  let {
    name,
    src = null,
    size = 40,
    presence = null,
    cut = 'var(--surface-200)'
  }: {
    name: string;
    src?: string | null;
    size?: number;
    presence?: PresenceKind | null;
    /** Colour of the cut-out ring around the presence shape. */
    cut?: string;
  } = $props();

  const dot = $derived(Math.max(10, Math.round(size * 0.36)));
  // A stored avatar can be missing (the bridge answers 404). Then the initials show.
  let failed = $state('');
</script>

<span class="avatar" style:width="{size}px" style:height="{size}px">
  {#if src && failed !== src}
    <img {src} alt="" onerror={() => (failed = src ?? '')} />
  {:else}
    <span class="fallback" style:background={tint(name)} style:font-size="{Math.round(size * 0.4)}px">
      {initials(name)}
    </span>
  {/if}
  {#if presence}
    <span class="badge">
      <Presence kind={presence} size={dot} {cut} pad={size >= 40 ? 3 : 2} />
    </span>
  {/if}
</span>

<style>
  .avatar {
    position: relative;
    display: inline-block;
    flex: none;
  }
  img,
  .fallback {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    display: grid;
    place-items: center;
    object-fit: cover;
  }
  .fallback {
    color: #f6f4ef;
    font-weight: 600;
    line-height: 1;
    user-select: none;
  }
  .badge {
    position: absolute;
    right: -4px;
    bottom: -4px;
    display: grid;
  }
</style>
