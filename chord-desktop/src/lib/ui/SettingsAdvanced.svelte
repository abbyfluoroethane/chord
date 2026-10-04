<script lang="ts">
  // Advanced: the connection, what the server offers, the stored data, and a reset.
  import { onMount } from 'svelte';
  import type { AppInfo, StorageInfo } from '$lib/chord/types';
  import SettingRow from './SettingRow.svelte';
  import SettingsServer from './SettingsServer.svelte';
  import { plainError } from './adapt';
  import { debugText, knownFeatures, RESET_FILE_KEYS, RESET_LOCAL_KEYS, uptimeText } from './advanceddata';
  import { api, live } from './bridge';
  import { copyText } from './clipboard';
  import { fileSize } from './format';
  import { settings } from './local';
  import { tick } from './now.svelte';
  import { session } from './session.svelte';
  import { ui } from './ui.svelte';

  // Sample data for the browser preview, which has no core.
  const SAMPLE_FEATURES = [
    'http://jabber.org/protocol/commands',
    'http://jabber.org/protocol/muc',
    'http://jabber.org/protocol/pubsub',
    'jabber:iq:register',
    'urn:xmpp:blocking',
    'urn:xmpp:bookmarks:1',
    'urn:xmpp:carbons:2',
    'urn:xmpp:csi:0',
    'urn:xmpp:http:upload:0',
    'urn:xmpp:mam:2',
    'urn:xmpp:push:0',
    'urn:xmpp:sid:0',
    'urn:xmpp:vendor:extra:1'
  ];
  const SAMPLE_STORAGE: StorageInfo = { database: 48_300_000, avatars: 2_100_000, emojiCache: 3_400_000 };
  const SAMPLE_APP: AppInfo = {
    version: '0.1.0',
    commit: 'preview',
    build: 0,
    channel: 'dev',
    os: 'preview',
    arch: 'browser',
    updater: 'none',
    flatpak: null
  };

  let features = $state<string[]>([]);
  let featuresLoaded = $state(false);
  let storage = $state<StorageInfo | null>(null);
  let app = $state<AppInfo | null>(null);

  const now = $derived(tick());
  const connected = $derived(session.state === 'connected');
  const names = $derived(knownFeatures(features));
  const status = $derived(
    {
      connected: 'Connected',
      connecting: 'Connecting',
      reconnecting: 'Reconnecting',
      restoring: 'Connecting',
      'signed-out': 'Signed out'
    }[session.state]
  );

  async function loadFeatures() {
    try {
      features = live ? await (await api()).serverFeatures() : SAMPLE_FEATURES;
    } catch {
      features = [];
    }
    featuresLoaded = true;
  }

  async function loadStorage() {
    try {
      storage = live ? await (await api()).storageInfo() : SAMPLE_STORAGE;
    } catch {
      storage = null;
    }
  }

  onMount(() => {
    void loadStorage();
    void (async () => {
      try {
        app = live ? await (await api()).appInfo() : SAMPLE_APP;
      } catch {
        app = null;
      }
    })();
  });

  // The server answers a moment after the connection comes up, so ask again then.
  $effect(() => {
    if (session.state !== 'connected') {
      features = [];
      featuresLoaded = false;
      return;
    }
    void loadFeatures();
    const again = setTimeout(() => void loadFeatures(), 4000);
    return () => clearTimeout(again);
  });

  async function reconnect() {
    await session.retry();
  }

  async function clearCaches() {
    try {
      const freed = live ? await (await api()).clearCaches() : SAMPLE_STORAGE.emojiCache;
      ui.say(freed > 0 ? `Cleared ${fileSize(freed)}.` : 'The caches were empty.');
      await loadStorage();
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  function askClearHistory() {
    ui.confirm = {
      title: 'Clear local history',
      text: 'Delete the messages stored on this computer? Chord loads recent messages from the server again.',
      confirm: 'Clear history',
      onconfirm: () => void clearHistory()
    };
  }

  async function clearHistory() {
    try {
      if (live) await (await api()).clearHistory();
      else ui.say('The preview has no stored history.');
      if (live) location.reload();
    } catch (e) {
      ui.say(plainError(e), true);
    }
  }

  function copyDebug() {
    void copyText(
      debugText({
        version: app
          ? `${app.version} (${app.commit}) build ${app.build}, ${app.channel}` +
            (app.flatpak ? `, Flatpak ${app.flatpak.branch || 'unknown branch'}` : '')
          : 'unknown',
        os: app?.os ?? 'unknown',
        arch: app?.arch ?? '',
        server: session.host,
        status,
        features
      }),
      'Debug info copied.'
    );
  }

  function askReset() {
    ui.confirm = {
      title: 'Reset all settings',
      text: 'Set every setting back to its default? Your account, chats, and notes stay.',
      confirm: 'Reset settings',
      onconfirm: () => void reset()
    };
  }

  async function reset() {
    for (const key of RESET_FILE_KEYS) settings.set(key, undefined);
    try {
      for (const key of RESET_LOCAL_KEYS) localStorage.removeItem(key);
    } catch {
      /* storage blocked: nothing to remove */
    }
    await settings.flush();
    location.reload();
  }
</script>

<h2 class="section">Connection</h2>
<SettingRow title="Server">
  <span class="value mono">{session.host}</span>
</SettingRow>
<SettingRow title="Status">
  <span class="value">{status}</span>
</SettingRow>
{#if connected && session.connectedAt}
  <SettingRow title="Connected for">
    <span class="value">{uptimeText(now - session.connectedAt)}</span>
  </SettingRow>
{/if}
<SettingRow title="Reconnect now" hint="Sign in again with the saved password.">
  <button class="btn" disabled={!session.canRetry || session.splash} onclick={() => void reconnect()}
    >Reconnect</button
  >
</SettingRow>

<h2 class="section">Server features</h2>
{#if !connected}
  <p class="note">Connect to see what your server offers.</p>
{:else if !featuresLoaded}
  <p class="note" role="status">Loading…</p>
{:else if features.length === 0}
  <p class="note">Your server has not listed its features yet.</p>
{:else}
  <ul class="chips" aria-label="Features that your server offers">
    {#each names as name (name)}<li>{name}</li>{/each}
  </ul>
  <details>
    <summary>All {features.length} features</summary>
    <ul class="raw mono">
      {#each features as f (f)}<li>{f}</li>{/each}
    </ul>
  </details>
{/if}

<h2 class="section">Server commands</h2>
<SettingsServer />

<h2 class="section">Storage</h2>
<SettingRow
  title="Database"
  hint={storage ? `Includes ${fileSize(storage.avatars)} of avatars.` : ''}
>
  <span class="value">{storage ? fileSize(storage.database) : '…'}</span>
</SettingRow>
<SettingRow title="Drawn emoji">
  <span class="value">{storage ? fileSize(storage.emojiCache) : '…'}</span>
</SettingRow>
<SettingRow title="Clear all caches" hint="Link previews and drawn emoji load again when needed.">
  <button class="btn" onclick={() => void clearCaches()}>Clear caches</button>
</SettingRow>
<SettingRow title="Clear local history" hint="Deletes messages on this computer, not on the server.">
  <button class="btn btn-danger" onclick={askClearHistory}>Clear history</button>
</SettingRow>

<h2 class="section">Troubleshooting</h2>
<SettingRow title="Copy debug info" hint="Holds version, system, server, and features, but no address.">
  <button class="btn" onclick={copyDebug}>Copy</button>
</SettingRow>
<SettingRow title="Reset all settings">
  <button class="btn btn-danger" onclick={askReset}>Reset settings</button>
</SettingRow>

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
  .value {
    color: var(--ink-muted);
  }
  .mono {
    font-family: var(--font-mono);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0 0 var(--space-3);
    padding: 0;
    list-style: none;
  }
  .chips li {
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--surface-300, var(--hover));
    border: 1px solid var(--line);
    font-size: 13px;
    line-height: 20px;
  }
  summary {
    cursor: pointer;
    color: var(--ink-muted);
  }
  .raw {
    margin: var(--space-2) 0 0;
    padding: var(--space-3);
    list-style: none;
    background: var(--surface-200);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 18px;
    overflow-wrap: anywhere;
    user-select: text;
  }
</style>
