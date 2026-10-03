<script lang="ts">
  // My account. Two tabs. "Profile" holds the preview as others see it, the profile fields,
  // the avatar, and the availability at sign-in. "Account" holds the password, the other
  // devices, the pin of the server certificate, the saved password, the contacts export,
  // and the deletion of the account.
  import { onMount } from 'svelte';
  import type { CertStatus, Device } from '$lib/chord/types';
  import Avatar from './Avatar.svelte';
  import ChangePasswordModal from './ChangePasswordModal.svelte';
  import CopyAddress from './CopyAddress.svelte';
  import DeleteAccountModal from './DeleteAccountModal.svelte';
  import Segmented from './Segmented.svelte';
  import SettingRow from './SettingRow.svelte';
  import { app } from './app.svelte';
  import { api, live } from './bridge';
  import { plainError } from './adapt';
  import { resizeAvatar } from '../chord/avatarResize';
  import { contactsStore } from './contacts.svelte';
  import { toCsv, toVCards } from './contactsexport';
  import { tint } from './format';
  import { bannerColor } from './imagecolor';
  import { prefs } from './prefs.svelte';
  import { MAX_STATUS, type SignInShow } from './prefsdata';
  import { presenceKind, presenceLabel } from './types';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';

  type Tab = 'profile' | 'account';
  let tab = $state<Tab>('profile');

  // The drafts of the profile fields. The preview shows them before they are saved.
  let name = $state('');
  let fullName = $state('');
  let pronouns = $state('');
  let website = $state('');
  let about = $state('');
  let file = $state<HTMLInputElement>();
  let error = $state('');
  let saving = $state(false);
  let passwordOpen = $state(false);
  let deleteOpen = $state(false);
  let cert = $state<CertStatus | null>(null);
  let certError = $state('');
  let devices = $state<Device[]>([]);
  let devicesError = $state('');

  function reset() {
    name = app.me.name;
    fullName = app.myProfile.fullName;
    pronouns = app.myProfile.pronouns;
    website = app.myProfile.website;
    about = app.myProfile.about;
    error = '';
  }

  onMount(() => {
    reset();
    void loadCert();
    void loadDevices();
    // Other clients sign in and out while the page is open.
    const timer = setInterval(() => void loadDevices(), 15000);
    return () => clearInterval(timer);
  });

  // The banner takes its colour from the avatar, as in the profile card.
  let banner = $state<string | null>(null);
  $effect(() => {
    const src = app.me.avatar;
    banner = null;
    if (!src) return;
    let current = true;
    void bannerColor(src).then((c) => {
      if (current) banner = c;
    });
    return () => {
      current = false;
    };
  });

  const shownName = $derived(name.trim() || app.me.name);

  // --- profile fields -----------------------------------------------

  const nameChanged = $derived(name.trim() !== app.me.name);
  const fieldsChanged = $derived(
    fullName.trim() !== app.myProfile.fullName ||
      pronouns.trim() !== app.myProfile.pronouns ||
      website.trim() !== app.myProfile.website ||
      about.trim() !== app.myProfile.about
  );
  const changed = $derived((nameChanged || fieldsChanged) && name.trim().length > 0);

  // Maps to api.setNickname(name) and api.setProfile(edit): the nickname (XEP-0172) and
  // the vCard4 (XEP-0292) in our own PEP.
  async function save() {
    if (!changed || saving) return;
    const site = website.trim();
    if (site && !/^https?:\/\/\S+$/i.test(site)) {
      error = 'The website must start with http:// or https://.';
      return;
    }
    error = '';
    saving = true;
    try {
      if (live) {
        if (nameChanged) await (await api()).setNickname(name.trim());
        if (fieldsChanged) {
          await (await api()).setProfile({
            fullName: fullName.trim(),
            about: about.trim(),
            website: site,
            pronouns: pronouns.trim()
          });
        }
      }
      app.me.name = name.trim();
      app.myProfile = {
        fullName: fullName.trim(),
        about: about.trim(),
        website: site,
        pronouns: pronouns.trim()
      };
      reset();
      ui.say('Profile saved.');
    } catch (e) {
      error = plainError(e);
    } finally {
      saving = false;
    }
  }

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

  // --- availability at sign-in --------------------------------------

  const showOptions: { value: SignInShow; label: string }[] = [
    { value: 'last', label: 'Last used' },
    { value: 'chat', label: 'Available' },
    { value: 'away', label: 'Away' },
    { value: 'dnd', label: 'Do not disturb' }
  ];

  // --- other devices ------------------------------------------------

  const sampleDevices: Device[] = [
    { resource: 'chord-5f21', show: null, status: null, priority: 0 },
    { resource: 'phone', show: 'away', status: 'On the train', priority: 0 },
    { resource: 'laptop', show: 'dnd', status: null, priority: 0 }
  ];

  // Maps to api.ownDevices(): the resources of our account that have a presence.
  async function loadDevices() {
    if (!live) {
      devices = sampleDevices;
      return;
    }
    try {
      devices = await (await api()).ownDevices();
      devicesError = '';
    } catch (e) {
      devicesError = plainError(e);
    }
  }

  const thisResource = $derived(live ? session.resource : sampleDevices[0].resource);
  // This session comes first, even before the server has sent its presence back.
  const shownDevices = $derived.by(() => {
    const here = devices.find((d) => d.resource === thisResource);
    const mine: Device = here ?? { resource: thisResource, show: null, status: null, priority: 0 };
    const rest = devices.filter((d) => d.resource !== thisResource);
    return (thisResource ? [mine, ...rest] : rest).map((d) => ({
      ...d,
      here: d.resource === thisResource
    }));
  });

  function deviceLabel(d: Device): string {
    return presenceLabel[presenceKind(true, d.show)];
  }

  // --- certificate and saved password -------------------------------

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
      ui.say(plainError(e), true);
    }
  }

  // --- contacts export ----------------------------------------------

  // Maps to api.saveText(name, text): the system dialog asks where to save. In the browser
  // preview the file downloads.
  async function exportContacts(kind: 'vcf' | 'csv') {
    const list = contactsStore.contacts;
    if (!list.length) {
      ui.say('You have no contacts to export.');
      return;
    }
    const text = kind === 'vcf' ? toVCards(list) : toCsv(list);
    const fileName = `chord-contacts.${kind}`;
    try {
      if (live) {
        if (await (await api()).saveText(fileName, text)) ui.say('Contacts exported.');
        return;
      }
      const type = kind === 'vcf' ? 'text/vcard' : 'text/csv';
      const url = URL.createObjectURL(new Blob([text], { type }));
      const a = document.createElement('a');
      a.href = url;
      a.download = fileName;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      ui.say('Contacts exported.');
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }
</script>

<div class="tabs">
  <Segmented
    value={tab}
    label="My account"
    options={[
      { value: 'profile', label: 'Profile' },
      { value: 'account', label: 'Account' }
    ]}
    onchange={(v) => (tab = v)}
  />
</div>

{#if tab === 'profile'}
  <section class="card preview" aria-label="Preview">
    <h2 class="section">Preview</h2>
    <div class="pc">
      <div class="banner" style:background={banner ?? tint(shownName)}></div>
      <div class="pav">
        <Avatar
          name={shownName}
          src={app.me.avatar}
          size={80}
          presence={presenceKind(true, app.me.show)}
          cut="var(--surface-300)"
        />
      </div>
      <div class="pid">
        <span class="pname">{shownName}</span>
        <span class="mono meta">{app.me.address}</span>
        {#if pronouns.trim()}<span class="meta">{pronouns.trim()}</span>{/if}
        {#if app.me.status}<span class="pstatus">{app.me.status}</span>{/if}
        {#if about.trim()}
          <p class="pabout">{about.trim()}</p>
        {/if}
      </div>
    </div>
  </section>

  <section class="card" aria-label="Profile">
    <h2 class="section">Profile</h2>
    <div class="avatar-row">
      <Avatar name={shownName} src={app.me.avatar} size={48} cut="var(--surface-200)" />
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
        <button class="btn btn-ghost" onclick={removeAvatar}>Remove avatar</button>
      {/if}
    </div>

    <div class="grid">
      <div class="field">
        <label for="display-name">Display name</label>
        <input id="display-name" class="input" maxlength="40" bind:value={name} />
      </div>
      <div class="field">
        <label for="full-name">Full name</label>
        <input id="full-name" class="input" maxlength="128" bind:value={fullName} />
      </div>
      <div class="field">
        <label for="pronouns">Pronouns</label>
        <input id="pronouns" class="input" maxlength="64" bind:value={pronouns} />
      </div>
      <div class="field">
        <label for="website">Website</label>
        <input
          id="website"
          class="input"
          maxlength="512"
          placeholder="https://"
          inputmode="url"
          bind:value={website}
        />
      </div>
    </div>
    <div class="field">
      <label for="about">About me</label>
      <textarea id="about" class="input about" rows="4" maxlength="500" bind:value={about}></textarea>
      <span class="meta count">{about.length} of 500</span>
    </div>
    {#if error}<span class="err" role="alert">{error}</span>{/if}
    <div class="bar">
      <span class="meta">Your contacts see these fields.</span>
      <div class="buttons">
        <button class="btn btn-ghost" disabled={!(nameChanged || fieldsChanged) || saving} onclick={reset}
          >Reset</button
        >
        <button class="btn btn-primary" disabled={!changed || saving} onclick={() => void save()}
          >Save changes</button
        >
      </div>
    </div>
  </section>

  <section class="card" aria-label="Sign-in">
    <h2 class="section">At sign-in</h2>
    <SettingRow title="Availability" hint="Chord sets it when you sign in.">
      <Segmented
        value={prefs.signInShow}
        label="Availability at sign-in"
        options={showOptions}
        onchange={(v) => prefs.set('signInShow', v)}
      />
    </SettingRow>
    <SettingRow title="Status text" hint="Empty keeps the text from your last session.">
      <input
        class="input status"
        aria-label="Status text at sign-in"
        maxlength={MAX_STATUS}
        placeholder="No text"
        value={prefs.signInStatus}
        onchange={(e) => prefs.set('signInStatus', e.currentTarget.value.trim())}
      />
    </SettingRow>
  </section>
{:else}
  <section class="card" aria-label="Sign-in details">
    <h2 class="section">Sign-in</h2>
    <div class="field">
      <span class="label">Address</span>
      <CopyAddress address={app.me.address} />
    </div>
    <div class="row">
      <div class="text">
        <span class="label">Password</span>
      </div>
      <button class="btn" onclick={() => (passwordOpen = true)}>Change password</button>
    </div>
    {#if live && session.hasSavedPassword}
      <div class="row">
        <div class="text">
          <span class="label">Saved password</span>
          <span class="meta">Stored in the system keychain.</span>
        </div>
        <button class="btn" onclick={() => void forgetSaved()}>Forget it now</button>
      </div>
    {/if}
  </section>

  <section class="card" aria-label="Devices">
    <h2 class="section">Devices</h2>
    <ul class="devices">
      {#each shownDevices as d (d.resource)}
        <li class="device">
          <span class="dot {presenceKind(true, d.show)}" aria-hidden="true"></span>
          <div class="text">
            <span class="label mono">{d.resource}</span>
            <span class="meta">{d.status || deviceLabel(d)}</span>
          </div>
          {#if d.here}<span class="here">This device</span>{/if}
        </li>
      {:else}
        <li class="meta">No device is online.</li>
      {/each}
    </ul>
    {#if devicesError}<span class="err" role="alert">{devicesError}</span>{/if}
  </section>

  <section class="card" aria-label="Server certificate">
    <h2 class="section">Server certificate</h2>
    <span class="meta">Stop signing in if the server certificate changes.</span>
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
          <span class="meta">Trusted by you, not by your system.</span>
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

  <section class="card" aria-label="Contacts export">
    <h2 class="section">Contacts</h2>
    <div class="row">
      <div class="text">
        <span class="label">Export your contacts</span>
        <span class="meta">{contactsStore.contacts.length} contacts, with their addresses.</span>
      </div>
      <div class="buttons">
        <button class="btn" onclick={() => void exportContacts('vcf')}>vCard file</button>
        <button class="btn" onclick={() => void exportContacts('csv')}>CSV file</button>
      </div>
    </div>
  </section>

  <section class="card danger" aria-label="Delete account">
    <div class="row">
      <div class="text">
        <span class="label">Delete account</span>
        <span class="meta">This cannot be undone.</span>
      </div>
      <button class="btn btn-danger" onclick={() => (deleteOpen = true)}>Delete account</button>
    </div>
  </section>
{/if}

{#if passwordOpen}<ChangePasswordModal onclose={() => (passwordOpen = false)} />{/if}
{#if deleteOpen}<DeleteAccountModal onclose={() => (deleteOpen = false)} />{/if}

<style>
  .tabs {
    margin-bottom: var(--space-4);
  }
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
  .section {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .preview {
    padding-bottom: var(--space-6);
  }
  .pc {
    width: 340px;
    max-width: 100%;
    overflow: hidden;
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    padding-bottom: var(--space-4);
  }
  .banner {
    height: 72px;
    border-bottom: 1px solid var(--line);
  }
  .pav {
    width: fit-content;
    margin: -44px 0 0 var(--space-4);
    padding: 4px;
    border-radius: 50%;
    background: var(--surface-300);
  }
  .pid {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: var(--space-2) var(--space-4) 0;
    min-width: 0;
  }
  .pname {
    max-width: 100%;
    overflow-wrap: anywhere;
    font-size: 20px;
    line-height: 26px;
    font-weight: 600;
    color: var(--brand-ink);
  }
  .pstatus {
    margin-top: var(--space-1);
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  .pabout {
    margin: var(--space-2) 0 0;
    max-width: 100%;
    font-size: 14px;
    line-height: 20px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .avatar-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .about {
    resize: vertical;
    min-height: 80px;
    font: inherit;
  }
  .count {
    align-self: flex-end;
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .buttons {
    display: flex;
    gap: var(--space-2);
  }
  .err {
    color: var(--danger);
    font-size: 14px;
  }
  .status {
    width: 220px;
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
    gap: var(--space-3);
  }
  .device {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .device .text {
    flex: 1;
  }
  .here {
    padding: 2px var(--space-2);
    background: var(--surface-300);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 16px;
    color: var(--ink-muted);
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--ink-muted);
  }
  .dot.online {
    background: var(--success, #3ba55d);
  }
  .dot.away {
    background: var(--warning, #faa61a);
  }
  .dot.dnd {
    background: var(--danger);
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
