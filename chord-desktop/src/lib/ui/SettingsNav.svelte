<script lang="ts">
  // Left navigation of the settings overlay, 218px.
  import type { SettingsPage } from './types';
  import { ui } from './ui.svelte';

  let { onlogout }: { onlogout: () => void } = $props();

  const groups: { label: string; pages: { id: SettingsPage; label: string }[] }[] = [
    {
      label: 'User settings',
      pages: [
        { id: 'account', label: 'My account' },
        { id: 'privacy', label: 'Privacy' },
        { id: 'server', label: 'Server' }
      ]
    },
    {
      label: 'App settings',
      pages: [
        { id: 'notifications', label: 'Notifications' },
        { id: 'appearance', label: 'Appearance' },
        { id: 'keybinds', label: 'Keybinds' }
      ]
    }
  ];
</script>

<nav aria-label="Settings">
  {#each groups as g (g.label)}
    <h2 class="group">{g.label}</h2>
    {#each g.pages as p (p.id)}
      <button
        class="item"
        class:on={ui.settingsPage === p.id}
        aria-current={ui.settingsPage === p.id ? 'page' : undefined}
        onclick={() => (ui.settingsPage = p.id)}>{p.label}</button
      >
    {/each}
    <hr />
  {/each}
  <button
    class="item"
    class:on={ui.settingsPage === 'about'}
    aria-current={ui.settingsPage === 'about' ? 'page' : undefined}
    onclick={() => (ui.settingsPage = 'about')}>About</button
  >
  <hr />
  <button class="item danger" onclick={onlogout}>Log out</button>
</nav>

<style>
  nav {
    display: flex;
    flex-direction: column;
    width: 218px;
    flex: none;
    padding: 60px 6px 60px 20px;
  }
  .group {
    margin: 0;
    padding: 6px 10px;
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .item {
    height: 32px;
    margin-bottom: 2px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    color: var(--ink-muted);
    text-align: left;
    font-weight: 500;
    transition:
      background var(--dur-fast),
      color var(--dur-fast);
  }
  .item:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .item.on {
    background: var(--selected);
    color: var(--ink);
  }
  .item.danger {
    color: var(--danger);
  }
  .item.danger:hover {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }
  hr {
    width: calc(100% - 20px);
    margin: var(--space-2) 10px;
    border: 0;
    border-top: 1px solid var(--line);
  }
</style>
