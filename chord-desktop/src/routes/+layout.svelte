<script lang="ts">
  import '$lib/theme/fonts.css';
  import '$lib/theme/tokens.css';
  import '$lib/theme/base.css';
  import '$lib/theme/tooltip.css';
  import { onMount } from 'svelte';
  import { theme } from '$lib/theme/theme.svelte';

  let { children } = $props();

  onMount(() => theme.load());
</script>

{@render children()}
