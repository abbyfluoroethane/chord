<script lang="ts">
  import '$lib/theme/fonts.css';
  import '$lib/theme/tokens.css';
  import '$lib/theme/base.css';
  import '$lib/theme/tooltip.css';
  import { onMount } from 'svelte';
  import { theme } from '$lib/theme/theme.svelte';
  import { installContextGuard } from '$lib/ui/contextmenu.svelte';

  let { children } = $props();

  onMount(() => {
    theme.load();
    // No webview menu, except in text fields. The app draws its own menus.
    return installContextGuard();
  });
</script>

{@render children()}
