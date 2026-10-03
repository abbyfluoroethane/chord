<script lang="ts">
  // Server commands: the ad-hoc commands (XEP-0050) that the server of the account offers.
  // Most people never need them. A server owner uses them for admin tasks.
  import { onMount } from 'svelte';
  import type { CommandItem } from '$lib/chord/types';
  import { sampleCommands } from '$lib/fixtures/forms';
  import CommandModal from './CommandModal.svelte';
  import SettingRow from './SettingRow.svelte';
  import { app } from './app.svelte';
  import { plainError } from './adapt';
  import { api, live } from './bridge';
  import { commandTitle, domainOf } from './forms';

  let items = $state<CommandItem[]>([]);
  let loading = $state(true);
  let error = $state('');
  let running = $state<CommandItem | null>(null);

  const server = $derived(domainOf(app.me.address));

  async function load() {
    loading = true;
    error = '';
    try {
      items = live ? await (await api()).listCommands(server) : sampleCommands;
    } catch (e) {
      error = plainError(e);
      items = [];
    } finally {
      loading = false;
    }
  }

  onMount(() => void load());
</script>

<section class="card" aria-label="Server commands">
  <p class="meta">
    Commands that {server || 'your server'} offers.
  </p>
  {#if loading}
    <p class="meta" role="status">Loading…</p>
  {:else if error}
    <p class="err" role="alert">{error}</p>
  {:else if items.length === 0}
    <p class="meta">Your server offers no commands.</p>
  {:else}
    {#each items as item (item.node)}
      <SettingRow title={commandTitle(item)} hint={item.name ? item.node : ''}>
        <button class="btn" onclick={() => (running = item)}>Run</button>
      </SettingRow>
    {/each}
  {/if}
  <div class="actions">
    <button class="btn btn-ghost" disabled={loading} onclick={() => void load()}>Refresh</button>
  </div>
</section>

{#if running}
  <CommandModal item={running} onclose={() => (running = null)} />
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-6);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  .card > p {
    margin: 0 0 var(--space-2);
  }
  .err {
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-2);
  }
</style>
