<script lang="ts">
  // My account: avatar, display name, address, password row, the pin of the server
  // certificate, the saved password, and the deletion of the account.
  import { onMount } from 'svelte';
  import type { CertStatus } from '$lib/chord/types';
  import Avatar from './Avatar.svelte';
  import ChangePasswordModal from './ChangePasswordModal.svelte';
  import DeleteAccountModal from './DeleteAccountModal.svelte';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { plainError } from './adapt';
  import { resizeAvatar } from '../chord/avatarResize';
  import { presenceKind } from './types';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';

  let name = $state('');
  let file = $state<HTMLInputElement>();
  let error = $state('');
  let passwordOpen = $state(false);
  let deleteOpen = $state(false);
  let cert = $state<CertStatus | null>(null);
  let certError = $state('');

  onMount(() => {
    name = app.me.name;
    void loadCert();
  });

  // Maps to api.certStatus(account): the pin, and the fingerprint of the last connection.
  async function loadCert() {
    if (!live) {
      cert = {
        pinned: null,
        trustUntrusted: false,
        observed: '3F:9A:0C:55:B2:7E:41:D8:6C:1B:E0:94:AA:27:5D:F3:08:C6:71:2B:9D:E4:50:1A:83:BF:66:D1:39:7C:0E:A5',
        problem: 'none'
      };
      return;
    }
    try {
      cert = await (await api()).certStatus(app.me.address);
    } catch (e) {
      certError = plainError(e);
    }
  }

  // Maps to api.certTrust(account): pins the certificate that the last login saw.
  async function pinCert() {
    certError = '';
    if (!live) {
      if (cert) cert = { ...cert, pinned: cert.observed };
      return;
    }
    try {
      cert = await (await api()).certTrust(app.me.address);
      ui.say('Certificate pinned.');
    } catch (e) {
      certError = plainError(e);
    }
  }

  // Maps to api.certClear(account).
  async function clearCert() {
    certError = '';
    if (!live) {
      if (cert) cert = { ...cert, pinned: null };
      return;
    }
    try {
      cert = await (await api()).certClear(app.me.address);
      ui.say('Certificate pin removed.');
    } catch (e) {
      certError = plainError(e);
    }
  }

  // Maps to api.forgetPassword(account).
  async function forgetSaved() {
    try {
      if (live) await (await api()).forgetPassword(app.me.address);
      session.hasSavedPassword = false;
      ui.say('Saved password forgotten.');
    } catch (e) {
      certError = plainError(e);
    }
  }

  const changed = $derived(name.trim() !== app.me.name && name.trim().length > 0);

  function pick() {
    error = '';
    const f = file?.files?.[0];
    if (!f) return;
    if (!f.type.startsWith('image/')) {
      error = 'Pick an image file.';
      return;
    }
    // The app shrinks it to 256 by 256 before it goes out. This only stops a huge file.
    if (f.size > 20 * 1024 * 1024) {
      error = 'The image is too big. Use one under 20 MB.';
      return;
    }
    if (file) file.value = '';
    if (!live) {
      app.me.avatar = URL.createObjectURL(f);
      return;
    }
    void publish(f);
  }

  // Maps to api.setAvatar(mime, bytes, width, height).
  async function publish(f: File) {
    try {
      // At most 256 by 256, as PNG or JPEG, under 64 KiB (XEP-0084).
      const small = await resizeAvatar(f);
      await (await api()).setAvatar(small.mime, small.bytes, small.width, small.height);
      app.me.avatar = URL.createObjectURL(new Blob([small.bytes as BlobPart], { type: small.mime }));
      ui.say('Avatar changed.');
    } catch (e) {
      error = plainError(e);
    }
  }

  // Maps to api.removeAvatar().
  async function removeAvatar() {
    if (!live) {
      app.me.avatar = null;
      return;
    }
    try {
      await (await api()).removeAvatar();
      app.me.avatar = null;
      ui.say('Avatar removed.');
    } catch (e) {
      error = plainError(e);
    }
  }

  // Maps to api.setNickname(name): publishes the nickname (XEP-0172) to our own PEP.
  async function save() {
    const next = name.trim();
    if (!live) {
      app.me.name = next;
      ui.say('Saved.');
      return;
    }
    error = '';
    try {
      await (await api()).setNickname(next);
      app.me.name = next;
      ui.say('Saved.');
    } catch (e) {
      error = plainError(e);
    }
  }
</script>

<section class="card" aria-label="Profile">
  <div class="top">
    <Avatar
      name={app.me.name}
      src={app.me.avatar}
      size={80}
      presence={presenceKind(true, app.me.show)}
      cut="var(--surface-200)"
    />
    <div class="who">
      <span class="title">{app.me.name}</span>
      <span class="mono meta">{app.me.address}</span>
      <div class="buttons">
        <input
          bind:this={file}
          class="sr-only"
          id="avatar-file"
          type="file"
          accept="image/*"
          tabindex="-1"
          onchange={pick}
        />
        <button class="btn" onclick={() => file?.click()}>Change avatar</button>
        {#if app.me.avatar}
          <button class="btn btn-ghost" onclick={removeAvatar}>Remove</button>
        {/if}
      </div>
      {#if error}<span class="err" role="alert">{error}</span>{/if}
    </div>
  </div>

  <div class="field">
    <label for="display-name">Display name</label>
    <div class="line">
      <input
        id="display-name"
        class="input grow"
        maxlength="40"
        bind:value={name}
      />
      <button class="btn btn-primary" disabled={!changed} onclick={save}>Save</button>
    </div>
    {#if live}
      <span class="meta">Your contacts whose apps read nicknames see this name.</span>
    {/if}
  </div>

  <div class="field">
    <label for="my-address">Address</label>
    <input id="my-address" class="input mono" readonly value={app.me.address} />
    <span class="meta">This is how people find you. You cannot change it.</span>
  </div>
</section>

<section class="card" aria-label="Password">
  <div class="row">
    <div class="text">
      <span class="label">Password</span>
      <span class="meta">Choose a new password for your account.</span>
    </div>
    <button class="btn" onclick={() => (passwordOpen = true)}>Change password</button>
  </div>
</section>

<section class="card" aria-label="Server certificate">
  <div class="text">
    <span class="label">Server certificate</span>
    <span class="meta">
      Pin the certificate of your server. Chord then stops before it sends your password if the
      server shows another certificate. Without a pin, Chord trusts any certificate that your
      system trusts.
    </span>
  </div>
  {#if cert?.observed}
    <div class="print-row">
      <span class="meta">This connection (SHA-256)</span>
      <code class="print">{cert.observed}</code>
    </div>
  {/if}
  {#if cert?.pinned}
    <div class="print-row">
      <span class="meta">Pinned</span>
      <code class="print">{cert.pinned}</code>
      {#if cert.trustUntrusted}
        <span class="meta">You chose to trust this certificate although your system does not.</span>
      {/if}
    </div>
  {/if}
  {#if certError}<span class="err" role="alert">{certError}</span>{/if}
  <div class="buttons">
    {#if cert?.pinned}
      <button class="btn" onclick={() => void clearCert()}>Stop pinning</button>
    {:else if cert?.observed}
      <button class="btn" onclick={() => void pinCert()}>Pin this certificate</button>
    {:else}
      <span class="meta">Sign in to see the certificate.</span>
    {/if}
  </div>
</section>

{#if live && session.hasSavedPassword}
  <section class="card" aria-label="Saved password">
    <div class="row">
      <div class="text">
        <span class="label">Saved password</span>
        <span class="meta">
          Chord keeps your password in the system keychain so it can sign you in. To sign out and
          forget it in one step, use Log out and tick the box.
        </span>
      </div>
      <button class="btn" onclick={() => void forgetSaved()}>Forget it now</button>
    </div>
  </section>
{/if}

<section class="card danger" aria-label="Delete account">
  <div class="row">
    <div class="text">
      <span class="label">Delete account</span>
      <span class="meta">Delete your account on the server. You cannot undo this.</span>
    </div>
    <button class="btn btn-danger" onclick={() => (deleteOpen = true)}>Delete account</button>
  </div>
</section>

{#if passwordOpen}<ChangePasswordModal onclose={() => (passwordOpen = false)} />{/if}
{#if deleteOpen}<DeleteAccountModal onclose={() => (deleteOpen = false)} />{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
    padding: var(--space-6);
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-6);
  }
  .who {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  .buttons {
    display: flex;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
  .line {
    display: flex;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .text {
    display: flex;
    flex-direction: column;
  }
  .label {
    font-weight: 500;
  }
  .print-row {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .print {
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    overflow-wrap: anywhere;
    user-select: all;
  }
  .danger {
    border-color: var(--danger);
  }
</style>
