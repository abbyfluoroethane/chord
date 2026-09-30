<script lang="ts">
  // Hover toolbar: react, reply, edit (your last message), more.
  import Ellipsis from 'lucide-svelte/icons/ellipsis';
  import Pencil from 'lucide-svelte/icons/pencil';
  import Reply from 'lucide-svelte/icons/reply';
  import SmilePlus from 'lucide-svelte/icons/smile-plus';
  import Icon from './Icon.svelte';
  import { tooltip } from './tooltip';

  let {
    own,
    onreact,
    onreply,
    onedit,
    onmore
  }: {
    own: boolean;
    onreact: (anchor: HTMLElement) => void;
    onreply: () => void;
    onedit: () => void;
    onmore: (e: MouseEvent) => void;
  } = $props();
</script>

<div class="toolbar" role="toolbar" aria-label="Message actions">
  <button aria-label="Add reaction" use:tooltip={{ text: 'Add reaction', side: 'top' }} onclick={(e) => onreact(e.currentTarget)}>
    <Icon icon={SmilePlus} size={16} />
  </button>
  <button aria-label="Reply" use:tooltip={{ text: 'Reply', side: 'top' }} onclick={onreply}>
    <Icon icon={Reply} size={16} />
  </button>
  {#if own}
    <button aria-label="Edit" use:tooltip={{ text: 'Edit', side: 'top' }} onclick={onedit}>
      <Icon icon={Pencil} size={16} />
    </button>
  {/if}
  <button aria-label="More" aria-haspopup="menu" use:tooltip={{ text: 'More', side: 'top' }} onclick={onmore}>
    <Icon icon={Ellipsis} size={16} />
  </button>
</div>

<style>
  .toolbar {
    display: flex;
    padding: 2px;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  button:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
