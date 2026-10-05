<script lang="ts">
  // Settings, Encryption: the OMEMO fingerprint of this device, the other devices of the
  // account with a Remove button, and the notice after a keys reset.
  import { onMount } from 'svelte';
  import Fingerprint from './Fingerprint.svelte';
  import { plainError } from './adapt';
  import { deviceName, trustLabel } from './omemodata';
  import { omemo } from './omemo.svelte';
  import { ui } from './ui.svelte';
  import type { OmemoDevice } from '$lib/chord/types';

  let devices = $state<OmemoDevice[]>([]);
  let error = $state('');
  let loaded = $state(false);

  const here = $derived(devices.find((d) => d.thisDevice) ?? null);
  const others = $derived(devices.filter((d) => !d.thisDevice));

  async function read() {
    try {
      devices = await omemo.ownDevices();
      error = '';
    } catch (e) {
      error = plainError(e);
    } finally {
      loaded = true;
    }
  }

  onMount(() => void read());

  let seen = omemo.changes.own ?? 0;
  $effect(() => {
    const now = omemo.changes.own ?? 0;
    if (now !== seen) {
      seen = now;
      void read();
    }
  });

  function ask(d: OmemoDevice) {
    ui.confirm = {
      title: 'Remove this device?',
      text: `${deviceName(d)} stops getting new encrypted messages. Add it again by signing in on it.`,
      confirm: 'Remove',
      onconfirm: () => {
        omemo
          .removeOwn(d.deviceId)
          .then(() => ui.say('Device removed.'))
          .catch((e) => ui.say(plainError(e), true));
      }
    };
  }
</script>

{#if omemo.keysReset}
  <section class="notice" role="status" aria-label="Keys reset">
    <p class="head">Your keys are new.</p>
    <p class="meta">
      Chord lost the old keys of this device. Contacts see a new device. Old encrypted messages stay
      unreadable.
    </p>
    <div><button class="btn" onclick={() => (omemo.keysReset = false)}>Dismiss</button></div>
  </section>
{/if}

<section class="card" aria-label="This device">
  <h2 class="section">This device</h2>
  {#if here}
    <div class="text">
      <span class="label">{deviceName(here)}</span>
      <span class="meta">Device {here.deviceId}</span>
    </div>
    <Fingerprint fingerprint={here.fingerprint} />
  {:else if loaded}
    <span class="meta">The keys of this device are not ready.</span>
  {:else}
    <span class="meta">Loading.</span>
  {/if}
</section>

<section class="card" aria-label="Other devices">
  <h2 class="section">Other devices</h2>
  <ul class="devices">
    {#each others as d (d.deviceId)}
      <li class="device">
        <div class="text">
          <span class="label">{deviceName(d)}</span>
          <span class="meta">{trustLabel(d.trust)}{d.active ? '' : '. Not active'}</span>
          <Fingerprint fingerprint={d.fingerprint} />
        </div>
        <button class="btn btn-danger" onclick={() => ask(d)}>Remove</button>
      </li>
    {:else}
      <li class="meta">{loaded ? 'No other device.' : 'Loading.'}</li>
    {/each}
  </ul>
  {#if error}<span class="err" role="alert">{error}</span>{/if}
</section>

<style>
  .card,
  .notice {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
    padding: var(--space-6);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  .notice {
    border-color: var(--away);
  }
  .notice p {
    margin: 0;
  }
  .head {
    font-weight: 600;
  }
  .section {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .label {
    font-weight: 500;
  }
  .devices {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .device {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
</style>
