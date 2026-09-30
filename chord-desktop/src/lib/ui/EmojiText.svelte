<script lang="ts">
  // Plain text that can hold emoji. Each emoji shows in the emoji pack that the user
  // chose (Settings > Appearance). The markdown renderer uses it for every text run.
  let { text }: { text: string } = $props();
</script>

{text}
