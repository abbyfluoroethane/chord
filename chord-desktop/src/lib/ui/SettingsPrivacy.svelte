<script lang="ts">
  // Privacy. XMPP has no "who can add you" switch. You approve each request.
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { contactsStore } from './contacts.svelte';
  import { linkPreviews } from './linkpreviews.svelte';
  import { api, live } from './bridge';
  import { prefs } from './prefs.svelte';
  import { ui } from './ui.svelte';

  /** Save the choice, then tell the running client. Rust reads the file at the next open. */
  function setShareInfo(v: boolean) {
    prefs.set('shareInfo', v);
    if (live) void api().then((b) => b.setShareInfo(v)).catch(() => {});
  }

</script>

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

<h2 class="section">Blocked addresses</h2>
{#if contactsStore.blocked.length}
  <div class="bar">
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
    {#each contactsStore.blocked as b (b.address)}
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
  .bar {
    display: flex;
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
