<script lang="ts">
  import Modal from './Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const mac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
  const mod = mac ? 'Cmd' : 'Ctrl';

  const rows: { keys: string[]; what: string }[] = [
    { keys: [mod, 'K'], what: 'Jump to a circle, channel, or DM' },
    { keys: ['Alt', 'Up'], what: 'Previous channel' },
    { keys: ['Alt', 'Down'], what: 'Next channel' },
    { keys: ['Alt', 'Shift', 'Up'], what: 'Previous channel with unread messages' },
    { keys: ['Alt', 'Shift', 'Down'], what: 'Next channel with unread messages' },
    { keys: ['Esc'], what: 'Mark this channel as read' },
    { keys: ['Up'], what: 'Edit your last message (in an empty composer)' },
    { keys: ['Enter'], what: 'Send a message' },
    { keys: ['Shift', 'Enter'], what: 'Start a new line' },
    { keys: [mod, '/'], what: 'Show this list' }
  ];
</script>

<Modal title="Keyboard shortcuts" {onclose}>
  <ul>
    {#each rows as r (r.what)}
      <li>
        <span>{r.what}</span>
        <span class="keys">
          {#each r.keys as k, i (i)}<kbd>{k}</kbd>{/each}
        </span>
      </li>
    {/each}
  </ul>
</Modal>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  .keys {
    display: flex;
    gap: var(--space-1);
    flex: none;
  }
  kbd {
    min-width: 24px;
    padding: 2px 6px;
    text-align: center;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 16px;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
</style>
