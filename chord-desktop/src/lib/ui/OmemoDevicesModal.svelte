<script lang="ts">
  // The OMEMO devices of a contact: fingerprint, label, trust, and the Verify and Distrust
  // buttons. Refresh asks the server for new keys.
  import { onMount } from 'svelte';
  import RefreshCw from 'lucide-svelte/icons/refresh-cw';
  import Fingerprint from './Fingerprint.svelte';
  import Icon from './Icon.svelte';
  import Modal from './Modal.svelte';
  import { contactsStore } from './contacts.svelte';
  import { plainError } from './adapt';
  import { deviceName, isBlocked, trustButtons, trustFor, trustLabel } from './omemodata';
  import { omemo } from './omemo.svelte';
  import type { OmemoDevice } from '$lib/chord/types';

  let { jid, onclose }: { jid: string; onclose: () => void } = $props();

  let devices = $state<OmemoDevice[]>([]);
  let loading = $state(true);
  let error = $state('');
  /** The fingerprint of the row that waits for the core. */
  let busy = $state<string | null>(null);

  const name = $derived(contactsStore.person(jid).name);

  async function read(refresh: boolean) {
    loading = true;
    error = '';
    try {
      devices = refresh ? await omemo.refresh(jid) : await omemo.devices(jid);
    } catch (e) {
      error = plainError(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => void read(true));

  // The core says that something changed: read the list again.
  let seen: number | null = null;
  $effect(() => {
    const now = omemo.changes[jid] ?? 0;
    const before = seen;
    seen = now;
    if (before !== null && before !== now) {
      void omemo.devices(jid).then((d) => (devices = d)).catch(() => undefined);
    }
  });

  async function decide(d: OmemoDevice, action: 'verify' | 'distrust') {
    if (!d.fingerprint) return;
    busy = d.fingerprint;
    error = '';
    try {
      await omemo.setTrust(jid, d.fingerprint, trustFor(action));
      devices = await omemo.devices(jid);
    } catch (e) {
      error = plainError(e);
    } finally {
      busy = null;
    }
  }
</script>

<Modal title="Devices of {name}" size="medium" {onclose}>
  <p class="meta">Compare a fingerprint with {name} on another channel. Then press Verify.</p>

  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <ul class="list" aria-busy={loading}>
    {#each devices as d (d.deviceId)}
      {@const b = trustButtons(d)}
      <li class="dev" class:off={!d.active}>
        <div class="top">
          <span class="label">{deviceName(d)}</span>
          <span class="trust" class:ok={d.trust === 'verified'} class:bad={isBlocked(d.trust)}>
            {trustLabel(d.trust)}
          </span>
          {#if !d.active}<span class="state">Not active</span>{/if}
        </div>
        <Fingerprint fingerprint={d.fingerprint} />
        <div class="buttons">
          <button
            class="btn"
            disabled={!b.verify || busy === d.fingerprint}
            onclick={() => void decide(d, 'verify')}>Verify</button
          >
          <button
            class="btn btn-danger"
            disabled={!b.distrust || busy === d.fingerprint}
            onclick={() => void decide(d, 'distrust')}>Distrust</button
          >
        </div>
      </li>
    {:else}
      <li class="meta">{loading ? 'Loading.' : 'No device yet.'}</li>
    {/each}
  </ul>

  {#snippet footer()}
    <button class="btn btn-ghost" disabled={loading} onclick={() => void read(true)}>
      <Icon icon={RefreshCw} size={16} /> Refresh
    </button>
    <button class="btn btn-primary" onclick={onclose}>Done</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 var(--space-3);
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .dev {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .dev.off {
    opacity: 0.7;
  }
  .top {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .label {
    font-weight: 600;
  }
  .trust,
  .state {
    padding: 0 var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 18px;
    color: var(--ink-muted);
  }
  .trust.ok {
    color: var(--online);
    border-color: var(--online);
  }
  .trust.bad {
    color: var(--danger);
    border-color: var(--danger);
  }
  .buttons {
    display: flex;
    gap: var(--space-2);
  }
</style>
