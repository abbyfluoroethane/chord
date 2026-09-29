<script lang="ts">
  // Our status: availability with its presence shape, and a status text. It opens above
  // the user panel button and is as wide as that button.
  import Check from 'lucide-svelte/icons/check';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import Popover from './Popover.svelte';
  import Presence from './Presence.svelte';
  import { app } from './app.svelte';
  import { presenceKind, presenceLabel, type Show } from './types';

  let { anchor, onclose }: { anchor: HTMLElement; onclose: () => void } = $props();

  const width = $derived(anchor.getBoundingClientRect().width);
  let text = $state(app.me.status ?? '');

  const choices: Show[] = ['chat', 'away', 'dnd'];

  function pick(show: Show) {
    app.setShow(show);
    onclose();
  }

  function save(e: SubmitEvent) {
    e.preventDefault();
    app.setStatus(text);
    onclose();
  }

  function clear() {
    text = '';
    app.setStatus(null);
  }
</script>

<Popover {anchor} {onclose} placement="top-end" label="Your status" role="dialog">
  <div class="menu" style:width="{width}px">
    <ul role="menu" aria-label="Availability">
      {#each choices as show (show)}
        {@const kind = presenceKind(true, show)}
        <li role="none">
          <button
            role="menuitemradio"
            aria-checked={app.me.show === show || (show === 'chat' && !app.me.show)}
            class="row"
            onclick={() => pick(show)}
          >
            <Presence {kind} size={10} label={false} />
            <span class="label">{presenceLabel[kind]}</span>
            {#if app.me.show === show || (show === 'chat' && !app.me.show)}
              <span class="check"><Icon icon={Check} size={16} /></span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    <div class="divider" role="separator"></div>

    <form class="status" onsubmit={save}>
      <label class="sr-only" for="status-text">Status</label>
      <input
        id="status-text"
        class="input"
        type="text"
        maxlength="128"
        placeholder="Set a status"
        autocomplete="off"
        bind:value={text}
      />
      {#if app.me.status}
        <button type="button" class="clear" aria-label="Clear status" onclick={clear}>
          <Icon icon={X} size={16} />
        </button>
      {/if}
    </form>
    <p class="hint">Press Enter to save.</p>
  </div>
</Popover>

<style>
  .menu {
    box-sizing: border-box;
    min-width: 0;
    padding: var(--space-1);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--ink);
    font-size: 14px;
    text-align: left;
    transition: background var(--dur-fast);
  }
  .row:hover,
  .row:focus-visible {
    background: var(--hover);
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .check {
    display: grid;
    color: var(--brand);
  }
  .divider {
    height: 1px;
    margin: var(--space-1) var(--space-1);
    background: var(--line);
  }
  .status {
    position: relative;
    display: flex;
    align-items: center;
    padding: var(--space-1);
  }
  .status .input {
    width: 100%;
    min-width: 0;
    padding-right: 32px;
  }
  .clear {
    position: absolute;
    right: 8px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
  }
  .clear:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .hint {
    margin: 0;
    padding: 0 var(--space-2) var(--space-1);
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
  }
</style>
