<script lang="ts">
  import { AVATAR_RETRY_MS, avatarTry } from '$lib/chord/avatars';
  import { initials, tint } from './format';
  import type { ComponentType } from 'svelte';
  import Icon from './Icon.svelte';
  import Presence from './Presence.svelte';
  import type { PresenceKind } from './types';

  let {
    name,
    src = null,
    size = 40,
    presence = null,
    cut = 'var(--surface-200)',
    icon = null
  }: {
    name: string;
    src?: string | null;
    size?: number;
    presence?: PresenceKind | null;
    /** Colour of the cut-out ring around the presence shape. */
    cut?: string;
    /** Shown on the tint in place of the initials, for example for a group chat. */
    icon?: ComponentType | null;
  } = $props();

  const dot = $derived(Math.max(10, Math.round(size * 0.36)));
  // A stored avatar can be missing (the bridge answers 404). Then the initials show.
  // The image can arrive after the first request, so a few more tries follow.
  let failed = $state('');
  let tries = $state({ src: '', n: 0 });
  const attempt = $derived(tries.src === src ? tries.n : 0);
  const shown = $derived(src ? avatarTry(src, attempt) : null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function onError() {
    failed = shown ?? '';
    if (!src || attempt >= AVATAR_RETRY_MS.length) return;
    const next = { src, n: attempt + 1 };
    clearTimeout(timer);
    timer = setTimeout(() => (tries = next), AVATAR_RETRY_MS[attempt]);
  }

  $effect(() => () => clearTimeout(timer));
</script>

<span class="avatar" style:width="{size}px" style:height="{size}px">
  {#if shown && failed !== shown}
    <img src={shown} alt="" onerror={onError} />
  {:else}
    <span class="fallback" style:background={tint(name)} style:font-size="{Math.round(size * 0.4)}px">
      {#if icon}<Icon {icon} size={Math.round(size * 0.55)} />{:else}{initials(name)}{/if}
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
