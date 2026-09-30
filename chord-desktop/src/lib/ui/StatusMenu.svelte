<script lang="ts">
  // Our status: availability with its presence shape, and a status: an optional emoji and
  // a text. It saves as "$EMOJI $TEXT". It opens above the user panel and is as wide as
  // the channel list.
  import { onMount } from 'svelte';
  import Check from 'lucide-svelte/icons/check';
  import SmilePlus from 'lucide-svelte/icons/smile-plus';
  import X from 'lucide-svelte/icons/x';
  import Emoji from './Emoji.svelte';
  import EmojiPicker from './EmojiPicker.svelte';
  import { joinStatus, splitStatus } from './emojisplit';
  import Icon from './Icon.svelte';
  import Popover from './Popover.svelte';
  import Presence from './Presence.svelte';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { ownLabel, presenceKind, type Show } from './types';

  let { anchor, onclose }: { anchor: HTMLElement; onclose: () => void } = $props();

  // The popover adds a 1px border on each side.
  const width = $derived(anchor.getBoundingClientRect().width - 2);
  const saved = splitStatus(app.me.status);
  let emoji = $state(saved.emoji);
  let text = $state(saved.text);
  let emojiButton = $state<HTMLButtonElement>();
  /** The picker opens beside the whole menu, so that it covers none of it. */
  let menuEl = $state<HTMLDivElement>();
  let picking = $state(false);

  // Invisible needs the invisible command (XEP-0186) or privacy lists (XEP-0016) on the
  // server. Hide the choice when the server has neither. Keep it while it is the current
  // choice, so that the user can leave it.
  let canHide = $state(true);
  onMount(() => {
    if (!live) return;
    void api()
      .then((b) => b.invisibleMethod())
      .then((method) => (canHide = method !== null))
      .catch(() => {
        /* Offline or unknown: keep the choice. */
      });
  });
  const choices = $derived<Show[]>(
    canHide || app.me.show === 'invisible' ? ['chat', 'away', 'dnd', 'invisible'] : ['chat', 'away', 'dnd']
  );

  function pick(show: Show) {
    app.setShow(show);
    onclose();
  }

  function save(e: SubmitEvent) {
    e.preventDefault();
    app.setStatus(joinStatus(emoji, text));
    onclose();
  }

  // The field wraps a long status and grows with it. Enter saves: a status has no
  // line breaks.
  let field = $state<HTMLTextAreaElement>();

  function grow() {
    if (!field) return;
    field.style.height = 'auto';
    // scrollHeight has no border: add the 1px border of each side.
    field.style.height = `${field.scrollHeight + 2}px`;
  }

  $effect(() => {
    void text;
    grow();
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      app.setStatus(joinStatus(emoji, text));
      onclose();
    }
  }

  function clear() {
    emoji = '';
    text = '';
    app.setStatus(null);
  }

  function pickEmoji(e: string) {
    emoji = e;
    picking = false;
    field?.focus();
  }
</script>

<Popover {anchor} {onclose} placement="top-end" label="Your status" role="dialog">
  <div class="menu" style:width="{width}px" bind:this={menuEl}>
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
            <span class="label">
              {ownLabel(show)}
              {#if show === 'invisible'}
                <span class="note">Contacts see you as offline.</span>
              {/if}
            </span>
            {#if app.me.show === show || (show === 'chat' && !app.me.show)}
              <span class="check"><Icon icon={Check} size={16} /></span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    <div class="divider" role="separator"></div>

    <form class="status" onsubmit={save}>
      <button
        type="button"
        class="emoji"
        bind:this={emojiButton}
        aria-label={emoji ? `Status emoji ${emoji}, change it` : 'Add a status emoji'}
        aria-expanded={picking}
        onclick={() => (picking = !picking)}
      >
        {#if emoji}<Emoji {emoji} />{:else}<Icon icon={SmilePlus} size={18} />{/if}
      </button>
      <label class="sr-only" for="status-text">Status</label>
      <textarea
        id="status-text"
        class="input"
        rows="1"
        maxlength="128"
        placeholder="Set a status"
        autocomplete="off"
        bind:this={field}
        bind:value={() => text, (v) => (text = v.replace(/\s*\n\s*/g, ' '))}
        onkeydown={keydown}
      ></textarea>
      {#if app.me.status || emoji}
        <button type="button" class="clear" aria-label="Clear status" onclick={clear}>
          <Icon icon={X} size={16} />
        </button>
      {/if}
    </form>
    <!-- Outside the form: the emoji buttons of the picker would submit it. -->
    {#if picking && menuEl}
      <EmojiPicker
        anchor={menuEl}
        placement="right-start"
        onpick={pickEmoji}
        onclose={() => (picking = false)}
      />
    {/if}
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
  .row:has(.note) {
    height: auto;
    min-height: 32px;
    padding-top: var(--space-1);
    padding-bottom: var(--space-1);
  }
  .note {
    display: block;
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
    white-space: normal;
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
    align-items: flex-start;
    padding: var(--space-1);
  }
  .status .input {
    width: 100%;
    min-width: 0;
    padding-right: 32px;
    box-sizing: border-box;
    height: auto;
    min-height: 40px;
    padding-top: 9px;
    padding-bottom: 9px;
    resize: none;
    overflow: hidden;
    overflow-wrap: anywhere;
    line-height: 20px;
  }
  /* The emoji button sits in the field, left of the text. */
  .emoji {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-md);
    color: var(--ink-muted);
    font-size: 16px;
    transition:
      background var(--dur-fast) var(--ease-out),
      color var(--dur-fast) var(--ease-out);
  }
  .emoji:hover,
  .emoji[aria-expanded='true'] {
    background: var(--hover);
    color: var(--ink);
  }
  .status .input {
    padding-left: 44px;
  }
  .clear {
    position: absolute;
    top: 10px;
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
