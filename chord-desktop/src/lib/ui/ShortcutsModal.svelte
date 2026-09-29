<script lang="ts">
  import Modal from './Modal.svelte';
  import ShortcutTable from './ShortcutTable.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<Modal title="Keyboard shortcuts" {onclose}>
  <ShortcutTable />
</Modal>
