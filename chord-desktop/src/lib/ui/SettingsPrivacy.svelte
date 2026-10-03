<script lang="ts">
  // Privacy. XMPP has no "who can add you" switch. You approve each request.
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { contactsStore } from './contacts.svelte';
  import { linkPreviews } from './linkpreviews.svelte';
  import Segmented from './Segmented.svelte';
  import { api, live } from './bridge';
  import { fileSize } from './format';
  import { prefs } from './prefs.svelte';
  import { privacyCache } from './privacycache.svelte';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';
  import type { ArchiveDefault } from '$lib/chord/api';

  /** Save the choice, then tell the running client. Rust reads the file at the next open. */
  function setShareInfo(v: boolean) {
    prefs.set('shareInfo', v);
    if (live) void api().then((b) => b.setShareInfo(v)).catch(() => {});
  }

  /** Save the choice, then tell the running client. Rust reads the file at the next open. */
  function setNotice(key: 'sendReadNotices' | 'sendTypingNotices', v: boolean) {
    prefs.set(key, v);
    if (live)
      void api()
        .then((b) => b.setNotices(prefs.sendReadNotices, prefs.sendTypingNotices))
        .catch(() => {});
  }

  const waits = [
    { value: '5', label: '5 min' },
    { value: '10', label: '10 min' },
    { value: '30', label: '30 min' }
  ];

  // What the server archives. `undefined` while unknown, `null` when it has no archive.
  let archive = $state<ArchiveDefault | null | undefined>(live ? undefined : 'roster');
  const archiveOptions: { value: ArchiveDefault; label: string }[] = [
    { value: 'always', label: 'Everyone' },
    { value: 'roster', label: 'Contacts' },
    { value: 'never', label: 'No one' }
  ];

  $effect(() => {
    if (!live || session.state !== 'connected') return;
    void api()
      .then((b) => b.archiveDefault())
      .then((v) => (archive = v))
      .catch(() => (archive = undefined));
  });

  async function setArchive(v: ArchiveDefault) {
    const before = archive;
    archive = v;
    if (!live) return;
    try {
      archive = await (await api()).setArchiveDefault(v);
    } catch {
      archive = before;
      ui.say('Could not change the archive', true);
    }
  }

  $effect(() => {
    void privacyCache.refresh();
  });

  // A long list gets a search box.
  let blockedQuery = $state('');
  const shownBlocked = $derived(
    contactsStore.blocked.filter((b) =>
      b.address.toLowerCase().includes(blockedQuery.trim().toLowerCase())
    )
  );

  const pages = (n: number) => `${n} ${n === 1 ? 'page' : 'pages'} saved.`;
</script>

<h2 class="section">Activity</h2>
<SettingRow title="Send read receipts" hint="Contacts see when you have read their messages.">
  <Toggle
    checked={prefs.sendReadNotices}
    label="Send read receipts"
    onchange={(v) => setNotice('sendReadNotices', v)}
  />
</SettingRow>
<SettingRow title="Send typing notices" hint="Contacts see when you are typing.">
  <Toggle
    checked={prefs.sendTypingNotices}
    label="Send typing notices"
    onchange={(v) => setNotice('sendTypingNotices', v)}
  />
</SettingRow>
<SettingRow title="Share when you are idle" hint="Contacts see that you have not used Chord for a while.">
  <Toggle
    checked={prefs.shareIdle}
    label="Share when you are idle"
    onchange={(v) => prefs.set('shareIdle', v)}
  />
</SettingRow>
{#if prefs.shareIdle}
  <SettingRow title="Idle after" hint="Time without input in Chord.">
    <Segmented
      label="Idle after"
      value={String(prefs.idleMinutes)}
      options={waits}
      onchange={(v) => prefs.set('idleMinutes', Number(v))}
    />
  </SettingRow>
{/if}

{#if archive}
  <h2 class="section">Message archive</h2>
  <SettingRow title="Keep messages on the server for" hint="Whose messages your server stores for you.">
    <Segmented label="Keep messages on the server for" value={archive} options={archiveOptions} onchange={setArchive} />
  </SettingRow>
{/if}

<h2 class="section">Link previews</h2>
<SettingRow
  title="Show link previews"
  hint="Fetch information from web pages to show embeds. The site can see your IP address."
>
  <Toggle
    checked={linkPreviews.enabled}
    label="Show link previews"
    onchange={(v) => linkPreviews.setEnabled(v)}
  />
</SettingRow>


<SettingRow
  title="Load files from people who are not contacts"
  hint="File attachments show as plain links when disabled."
>
  <Toggle
    checked={linkPreviews.strangers}
    label="Load files from people who are not contacts"
    onchange={(v) => linkPreviews.setStrangers(v)}
  />
</SettingRow>

<h2 class="section">GIFs</h2>
<SettingRow
  title="Show the GIF picker"
  hint="The GIF picker is powered by KLIPY, who can see what you search for and your IP address."
>
  <Toggle
    checked={prefs.gifPicker}
    label="Show the GIF picker"
    onchange={(v) => prefs.set('gifPicker', v)}
  />
</SettingRow>

<h2 class="section">Software information</h2>
<SettingRow
  title="Answer version and time requests"
  hint="Allow other clients to ask for the name of the app, its version, and your local time."
>
  <Toggle
    checked={prefs.shareInfo}
    label="Answer version and time requests"
    onchange={setShareInfo}
  />
</SettingRow>

<h2 class="section">Contact requests</h2>
<SettingRow title="Approve contact requests automatically">
  <Toggle
    checked={prefs.autoApprove}
    label="Approve contact requests automatically"
    onchange={(v) => prefs.set('autoApprove', v)}
  />
</SettingRow>

{#if privacyCache.info}
  <h2 class="section">Saved on this device</h2>
  <SettingRow title="Link preview cache" hint={pages(privacyCache.info.previewEntries)}>
    <button
      class="btn"
      disabled={privacyCache.info.previewEntries === 0}
      onclick={() => privacyCache.clear('previews')}>Clear</button
    >
  </SettingRow>
  <SettingRow
    title="Drawn emoji cache"
    hint={`${privacyCache.info.emojiFiles} files, ${fileSize(privacyCache.info.emojiBytes)}.`}
  >
    <button
      class="btn"
      disabled={privacyCache.info.emojiFiles === 0}
      onclick={() => privacyCache.clear('emoji')}>Clear</button
    >
  </SettingRow>
{/if}

<h2 class="section">Blocked addresses</h2>
{#if contactsStore.blocked.length}
  <div class="bar">
    {#if contactsStore.blocked.length > 8}
      <input
        class="input search"
        type="search"
        placeholder="Search blocked addresses"
        aria-label="Search blocked addresses"
        autocomplete="off"
        bind:value={blockedQuery}
      />
    {/if}
    <button
        class="btn"
        onclick={() =>
          (ui.confirm = {
            title: 'Unblock all',
            text: `Unblock ${contactsStore.blocked.length === 1 ? '1 address' : `${contactsStore.blocked.length} addresses`}? These people can write to you again.`,
            confirm: 'Unblock all',
            onconfirm: () => contactsStore.unblockAll()
          })}>Unblock all</button
      >
  </div>
  <ul class="blocked">
    {#each shownBlocked as b (b.address)}
      <li>
        <span class="mono">{b.address}</span>
        <button
          class="btn"
          aria-label="Unblock {b.address}"
          onclick={() => contactsStore.unblock(b.address)}>Unblock</button
        >
      </li>
    {/each}
  </ul>
  {#if !shownBlocked.length}
    <p class="note">No blocked address matches.</p>
  {/if}
{:else}
  <p class="note">You have not blocked anyone.</p>
{/if}

<style>
  .section {
    margin: var(--space-6) 0 var(--space-2);
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
  }
  .section:first-of-type {
    margin-top: 0;
  }
  .note {
    margin: 0 0 var(--space-3);
    color: var(--ink-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .search {
    flex: 1;
    min-width: 0;
  }
  .bar {
    display: flex;
    gap: var(--space-3);
    justify-content: flex-end;
    margin-bottom: var(--space-2);
  }
  .blocked li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--line);
  }
</style>
