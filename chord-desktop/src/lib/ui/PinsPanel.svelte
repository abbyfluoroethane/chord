<script lang="ts">
  // The pins of the open chat: a button in the chat header, and a list below it. A click on
  // a pin jumps to the message when the page has it.
  import Pin from 'lucide-svelte/icons/pin';
  import PinOff from 'lucide-svelte/icons/pin-off';
  import Icon from './Icon.svelte';
  import Popover from './Popover.svelte';
  import { app } from './app.svelte';
  import { stamp } from './format';
  import { pinLine } from './pins';
  import { pins } from './pins.svelte';
  import { jumpTo } from './jump';
  import { tooltip } from './tooltip';
  import { ui } from './ui.svelte';
  import type { Pin as PinItem } from '$lib/chord/types';

  let button = $state<HTMLButtonElement>();
  let open = $state(false);

  const chat = $derived(app.selectedJid);
  const list = $derived(pins.of(chat));

  // A new chat has other pins. Read them, and close the list of the old chat.
  $effect(() => {
    const jid = chat;
    open = false;
    void pins.load(jid);
  });

  function toggle() {
    open = !open;
    if (open) void pins.load(chat);
  }

  // The Cmd+P shortcut opens or closes the list.
  $effect(() => ui.onRequest('pins', toggle));

  function pick(pin: PinItem) {
    if (!pin.itemId) {
      ui.say('Chord could not find that message. The server does not have it.');
      return;
    }
    open = false;
    void jumpTo(pin.itemId);
  }
</script>

<button
  bind:this={button}
  class="icon"
  class:on={open}
  aria-label="Pinned messages"
  aria-expanded={open}
  use:tooltip={{ text: 'Pinned messages', side: 'bottom' }}
  onclick={toggle}
>
  <Icon icon={Pin} size={20} />
  {#if list.length > 0}<span class="count" aria-hidden="true">{list.length}</span>{/if}
</button>

{#if open && button}
  <Popover anchor={button} onclose={() => (open = false)} placement="bottom-end" label="Pinned messages">
    <div class="panel">
      {#if list.length === 0}
        <p class="note">No pinned messages. Use the message menu to pin one.</p>
      {:else}
        <ul>
          {#each list as pin (pin.chat + pin.key)}
            <li>
              <button class="pin" onclick={() => pick(pin)}>
                <span class="meta">
                  <span class="who">{pin.sender.split('@')[0]}</span>
                  <span class="when">{stamp(pin.timestamp)}</span>
                </span>
                <span class="text">{pinLine(pin)}</span>
              </button>
              <button
                class="unpin"
                aria-label="Unpin"
                use:tooltip={{ text: 'Unpin', side: 'top' }}
                onclick={() => void pins.remove(pin)}
              >
                <Icon icon={PinOff} size={16} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </Popover>
{/if}

<style>
  .icon {
    position: relative;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .icon:hover,
  .icon.on {
    color: var(--ink);
  }
  .icon:hover {
    background: var(--hover);
  }
  .count {
    position: absolute;
    right: 0;
    bottom: 0;
    min-width: 14px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--brand-soft);
    color: var(--brand-ink);
    font-size: 10px;
    font-weight: 600;
    line-height: 14px;
    text-align: center;
  }
  .panel {
    width: 340px;
    max-height: 420px;
    overflow-y: auto;
    padding: var(--space-1);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: flex-start;
    border-radius: var(--radius-sm);
  }
  li:hover {
    background: var(--hover);
  }
  .pin {
    flex: 1;
    min-width: 0;
    padding: var(--space-2);
    text-align: left;
  }
  .meta {
    display: flex;
    gap: var(--space-2);
    font-size: 12px;
    line-height: 16px;
  }
  .who {
    color: var(--ink);
    font-weight: 600;
  }
  .when {
    color: var(--ink-muted);
  }
  .text {
    display: block;
    color: var(--ink);
    font-size: 14px;
    line-height: 20px;
    overflow-wrap: anywhere;
  }
  .unpin {
    display: grid;
    place-items: center;
    flex: none;
    width: 28px;
    height: 28px;
    margin: var(--space-1);
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
  }
  .unpin:hover {
    color: var(--ink);
    background: var(--hover);
  }
  .note {
    margin: 0;
    padding: var(--space-3);
    color: var(--ink-muted);
    font-size: 14px;
  }
</style>
