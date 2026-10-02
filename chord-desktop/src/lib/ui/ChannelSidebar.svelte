<script lang="ts">
  // Channel sidebar: space header, channel list (or DMs on Home), user panel.
  import Plus from 'lucide-svelte/icons/plus';
  import Search from 'lucide-svelte/icons/search';
  import ChannelRow from './ChannelRow.svelte';
  import CircleDialog from './CircleDialog.svelte';
  import CircleHeader from './CircleHeader.svelte';
  import ContactsRow from './ContactsRow.svelte';
  import Icon from './Icon.svelte';
  import UserPanel from './UserPanel.svelte';
  import { app, HOME } from './app.svelte';
  import { groupByCategory } from './categories';
  import { leaveGroup } from './menus';
  import { isGroup } from './types';
  import { tooltip } from './tooltip';
  import { ui } from './ui.svelte';

  let creating = $state(false);
  const isHome = $derived(app.selectedSpace === HOME);
  // A space shows its channels under the category headers. Home keeps one flat list.
  const groups = $derived(
    isHome ? [{ name: null, items: app.spaceChannels }] : groupByCategory(app.spaceChannels)
  );
</script>

<aside class="sidebar" aria-label={isHome ? 'Messages' : 'Channels'}>
  {#if isHome}
    <button class="header find" onclick={() => (ui.switcherOpen = true)}>
      <Icon icon={Search} size={16} />
      <span>Find or start a chat</span>
      <kbd>Ctrl K</kbd>
    </button>
  {:else if app.currentSpace}
    <CircleHeader name={app.currentSpace.name} space={app.selectedSpace} />
  {/if}

  <div class="scroll">
    {#if isHome}<ContactsRow />{/if}
    <div class="group">
      <span class="label">{isHome ? 'Messages' : 'Channels'}</span>
      {#if !isHome}
        <button
          class="add"
          aria-label="Create channel"
          use:tooltip={{ text: 'Create channel', side: 'top' }}
          onclick={() => (creating = true)}
        >
          <Icon icon={Plus} size={16} />
        </button>
      {/if}
    </div>
    {#each groups as g (g.name ?? '')}
      {#if g.name}
        <div class="group category"><span class="label">{g.name}</span></div>
      {/if}
      <ul>
        {#each g.items as c (c.jid)}
          <li>
            <ChannelRow
              channel={c}
              selected={c.jid === app.selectedJid && !app.showContacts}
              onclick={() => app.selectChannel(c.jid)}
              onclose={c.kind === 'dm'
                ? () => app.closeDm(c.jid)
                : isGroup(c) && c.joined
                  ? () => leaveGroup(c.jid, c.name)
                  : undefined}
            />
          </li>
        {/each}
      </ul>
    {/each}
  </div>

  <UserPanel />
</aside>

{#if creating}
  <CircleDialog kind="create-channel" space={app.selectedSpace} onclose={() => (creating = false)} />
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    width: var(--sidebar-width);
    background: var(--surface-side);
    border-right: 1px solid var(--line-strong);
    min-height: 0;
  }
  .header.find {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: var(--bar-height);
    flex: none;
    padding: 0 var(--space-4);
    border-bottom: 1px solid var(--line-strong);
    box-shadow: 0 1px 3px color-mix(in srgb, #000 14%, transparent);
    color: var(--ink);
    font-weight: 600;
    text-align: left;
    font-size: 14px;
  }
  .find:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .find span {
    flex: 1;
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 0 4px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-bottom: var(--space-4);
  }
  .group {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-4) var(--space-3) var(--space-1) var(--space-4);
  }
  .group.category {
    padding-top: var(--space-4);
  }
  .label {
    font-size: 12px;
    line-height: 16px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--ink) 88%, var(--surface-side));
  }
  .add {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
  }
  .add:hover {
    color: var(--ink);
    background: var(--hover);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
</style>
