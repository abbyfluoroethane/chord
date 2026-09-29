<script lang="ts">
  // Face of a circle: its picture, or initials on a tint.
  import { initials, tint } from './format';

  let { name, src = null, fill = false }: { name: string; src?: string | null; fill?: boolean } =
    $props();
</script>

{#if src}
  <img {src} alt="" class:fill />
{:else}
  <span class="face" class:fill style:background={tint(name)}>{initials(name)}</span>
{/if}

<style>
  img,
  .face {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .face {
    color: #f6f4ef;
  }
  .fill {
    border-radius: 6px;
  }
</style>
