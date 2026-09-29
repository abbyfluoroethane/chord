<script lang="ts">
  import ShortcutTable from './ShortcutTable.svelte';
</script>

<p class="note">These shortcuts are fixed for now. You cannot change them yet.</p>
<ShortcutTable />

<style>
  .note {
    margin: 0 0 var(--space-3);
    color: var(--ink-muted);
  }
</style>
