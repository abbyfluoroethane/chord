<script lang="ts">
  // Privacy. XMPP has no "who can add you" switch. You approve each request, or pre-approve addresses.
  import SettingRow from './SettingRow.svelte';
  import Toggle from './Toggle.svelte';
  import { contactsStore } from './contacts.svelte';
  import { linkPreviews } from './linkpreviews.svelte';
  import { api, live } from './bridge';
  import { prefs } from './prefs.svelte';

  let address = $state('');
  let result = $state<{ ok: boolean; text: string } | null>(null);

  /** Save the choice, then tell the running client. Rust reads the file at the next open. */
  function setShareInfo(v: boolean) {
    prefs.set('shareInfo', v);
    if (live) void api().then((b) => b.setShareInfo(v)).catch(() => {});
  }

  function preapprove(e: SubmitEvent) {
    e.preventDefault();
    if (!address.trim()) return;
    const r = contactsStore.preapprove(address);
    result = r.ok ? { ok: true, text: r.message } : { ok: false, text: r.error };
    if (r.ok) address = '';
  }
</script>

<h2 class="section">Link previews</h2>
<SettingRow
  title="Show link previews"
  hint="Chord fetches the page from this computer to show a preview. The site can see your IP address."
>
  <Toggle
    checked={linkPreviews.enabled}
    label="Show link previews"
    onchange={(v) => linkPreviews.setEnabled(v)}
  />
</SettingRow>


<h2 class="section">GIFs</h2>
<SettingRow
  title="Show the GIF picker"
  hint="The GIF search goes to KLIPY, so KLIPY sees what you search for and your IP address. A GIF that you send is a link: the people in the chat load it from KLIPY."
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
  hint="On by default. Other clients can ask for the name of this app, its version and your local time. Chord sends no operating system. Off, Chord answers neither request."
>
  <Toggle
    checked={prefs.shareInfo}
    label="Answer version and time requests"
    onchange={setShareInfo}
  />
</SettingRow>

<h2 class="section">Contact requests</h2>
<SettingRow
  title="Approve contact requests automatically"
  hint="Off by default. Requests wait in Pending until you accept them."
>
  <Toggle
    checked={prefs.autoApprove}
    label="Approve contact requests automatically"
    onchange={(v) => prefs.set('autoApprove', v)}
  />
</SettingRow>

<h2 class="section">Pre-approve</h2>
<p class="note">
  Pre-approve an address to accept its request before it arrives. Use it when you expect a request
  from someone.
</p>
<form onsubmit={preapprove}>
  <div class="line">
    <input
      class="input grow mono"
      aria-label="Address to pre-approve"
      placeholder="name@chord.example"
      spellcheck="false"
      bind:value={address}
      oninput={() => (result = null)}
    />
    <button class="btn btn-primary" type="submit" disabled={!address.trim()}>Pre-approve</button>
  </div>
  <p class="result" class:ok={result?.ok} class:err={result && !result.ok} role="status">
    {result?.text ?? ''}
  </p>
</form>
{#if contactsStore.preapproved.length}
  <ul class="plain">
    {#each contactsStore.preapproved as a (a)}
      <li class="mono">{a}</li>
    {/each}
  </ul>
{/if}

<h2 class="section">Blocked addresses</h2>
{#if contactsStore.blocked.length}
  <div class="bar">
    <button class="btn" onclick={() => contactsStore.unblockAll()}>Unblock all</button>
  </div>
  <ul class="blocked">
    {#each contactsStore.blocked as b (b.address)}
      <li>
        <span class="mono">{b.address}</span>
        <button class="btn" onclick={() => contactsStore.unblock(b.address)}>Unblock</button>
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
  .line {
    display: flex;
    gap: var(--space-2);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .result {
    min-height: 22px;
    margin: var(--space-2) 0 0;
    font-size: 14px;
  }
  .result.ok {
    color: var(--online);
  }
  .result.err {
    color: var(--danger);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .plain li {
    padding: var(--space-1) 0;
    color: var(--ink-muted);
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
