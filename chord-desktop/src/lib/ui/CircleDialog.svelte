<script module lang="ts">
  export type DialogKind =
    | 'invite'
    | 'settings'
    | 'create-channel'
    | 'notifications'
    | 'nickname'
    | 'leave';
</script>

<script lang="ts">
  // Dialogs opened from the space menu. Bridge calls: setNotificationLevel (each channel
  // of the space), changeNick, spaceJoinRequests, approveSpaceJoin, denySpaceJoin,
  // createChannel, leaveSpace, deleteSpace, and removeRoomFromSpace. The settings dialog of an
  // owner also calls spaceMembers, spaceConfig, configureSpace, setSpaceAvatar, setSpaceBanner,
  // removeSpaceMember and banSpaceMember.
  import { plainError, splitSpaceKey } from './adapt';
  import { failureNote } from './batch';
  import InviteList from './InviteList.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { ui } from './ui.svelte';
  import { api, live } from './bridge';
  import { spaceInviteLink } from './xmppuri';
  import type { NotificationLevel } from './types';
  import type { SpaceMember } from '$lib/chord/types';

  let { kind, space, onclose }: { kind: DialogKind; space: string; onclose: () => void } = $props();

  const circle = $derived(app.spaceOf(space));
  let text = $state('');
  let level = $state<NotificationLevel>('all');
  let copied = $state(false);
  let mute = $state(false);
  /** Join requests of the space. Only an owner gets them. */
  let requests = $state<{ jid: string; subid: string | null }[]>([]);
  let requestsNote = $state('');
  /**
   * I own the space: I can remove its channels, delete it, change its settings and manage its
   * members. The join requests and the members list both prove it. The preview owns all.
   */
  let owner = $state(!live);
  const channelsHere = $derived(app.channels.filter((c) => c.space === space && c.kind === 'channel'));

  // Maps to api.spaceJoinRequests(service, node).
  $effect(() => {
    if (!live || kind !== 'settings') return;
    const { service, node } = splitSpaceKey(space);
    void app
      .call((b) => b.spaceJoinRequests(service, node))
      .then((r) => {
        if (r.ok) {
          owner = true;
          requests = r.value;
          requestsNote = r.value.length ? '' : 'Nobody is waiting to join.';
        } else requestsNote = 'Only the owner of a space sees join requests.';
      });
  });

  let description = $state('');
  let savedDescription = '';
  let members = $state<SpaceMember[]>([]);
  let settingsError = $state('');
  let busy = $state(false);

  // Maps to api.spaceMembers(service, node) and api.spaceConfig(service, node). A service
  // refuses both to a person who is not the owner, and then the dialog stays read-only.
  $effect(() => {
    if (!live || kind !== 'settings') return;
    const { service, node } = splitSpaceKey(space);
    void (async () => {
      try {
        const b = await api();
        members = await b.spaceMembers(service, node);
        const config = await b.spaceConfig(service, node);
        description = config.find((f) => f.var === 'pubsub#description')?.value ?? '';
        savedDescription = description;
        owner = true;
      } catch {
        // Not the owner, or the service refused the read. The join request check above
        // decides too, so this does not clear what it found.
      }
    })();
  });

  // Maps to api.configureSpace(service, node, name, description).
  async function saveSettings() {
    if (!owner) return onclose();
    const { service, node } = splitSpaceKey(space);
    const name = text.trim();
    const newName = name && name !== circle?.name ? name : null;
    const newDescription = description.trim() !== savedDescription.trim() ? description.trim() : null;
    if (newName === null && newDescription === null) return onclose();
    busy = true;
    settingsError = '';
    try {
      await (await api()).configureSpace(service, node, newName, newDescription);
      ui.say('Space saved.');
      onclose();
    } catch (e) {
      settingsError = plainError(e);
    } finally {
      busy = false;
    }
  }

  // Maps to api.setSpaceAvatar and api.setSpaceBanner (image, width, height in pixels).
  async function pickImage(e: Event, banner: boolean) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    settingsError = '';
    const limit = banner ? 4 : 1;
    if (!f.type.startsWith('image/')) {
      settingsError = 'Pick an image file.';
      return;
    }
    if (f.size > limit * 1024 * 1024) {
      settingsError = `The image is too big. Use one under ${limit} MB.`;
      return;
    }
    const { service, node } = splitSpaceKey(space);
    busy = true;
    try {
      const bitmap = await createImageBitmap(f);
      const size = { w: bitmap.width, h: bitmap.height };
      bitmap.close();
      const bytes = new Uint8Array(await f.arrayBuffer());
      const b = await api();
      await (banner
        ? b.setSpaceBanner(service, node, f.type, bytes, size.w, size.h)
        : b.setSpaceAvatar(service, node, f.type, bytes, size.w, size.h));
      ui.say(banner ? 'Banner changed.' : 'Avatar changed.');
    } catch (err) {
      settingsError = plainError(err);
    } finally {
      busy = false;
    }
  }

  // Maps to api.removeSpaceMember and api.banSpaceMember, then api.spaceMembers.
  async function affiliate(jid: string, ban: boolean) {
    const { service, node } = splitSpaceKey(space);
    settingsError = '';
    try {
      const b = await api();
      await (ban ? b.banSpaceMember(service, node, jid) : b.removeSpaceMember(service, node, jid));
      members = await b.spaceMembers(service, node);
    } catch (err) {
      settingsError = plainError(err);
    }
  }

  const affiliationLabel: Record<string, string> = {
    owner: 'Owner',
    publisher: 'Publisher',
    'publish-only': 'Publisher',
    member: 'Member',
    outcast: 'Banned'
  };

  // Maps to api.approveSpaceJoin and api.denySpaceJoin.
  async function answer(jid: string, approve: boolean) {
    const { service, node } = splitSpaceKey(space);
    const r = await app.call((b) =>
      approve ? b.approveSpaceJoin(service, node, jid) : b.denySpaceJoin(service, node, jid)
    );
    if (r.ok) requests = requests.filter((x) => x.jid !== jid);
  }

  $effect.pre(() => {
    // Seed the fields once when the dialog opens.
    text =
      kind === 'nickname'
        ? (app.nickname[space] ?? app.me.name)
        : kind === 'settings'
          ? (circle?.name ?? '')
          : '';
    level = app.notifyLevel[space] ?? 'all';
    mute = level === 'nothing';
  });

  const link = $derived(circle ? spaceInviteLink(circle.service, circle.node) : '');
  const titles: Record<DialogKind, string> = {
    invite: 'Invite people',
    settings: 'Space settings',
    'create-channel': 'Create a channel',
    notifications: 'Notification settings',
    nickname: 'Change nickname',
    leave: 'Leave this space'
  };

  let picked = $state<string[]>([]);
  let sending = $state(false);

  async function sendInvites() {
    if (!picked.length || sending) return;
    sending = true;
    const n = await app.sendSpaceInvites(space, picked, link);
    sending = false;
    const note = failureNote(picked.length - n, picked.length, 'invite');
    if (note) ui.say(note, true);
    else ui.say(n === 1 ? 'Sent 1 invite.' : `Sent ${n} invites.`);
    onclose();
  }

  function save() {
    if (kind === 'invite') return void sendInvites();
    if (kind === 'create-channel') app.createChannel(space, text);
    else if (kind === 'nickname' && text.trim()) void app.changeNick(space, text.trim());
    else if (kind === 'notifications') void app.setCircleLevel(space, mute ? 'nothing' : level);
    else if (kind === 'settings' && live) return void saveSettings();
    else if (kind === 'settings' && circle && text.trim()) circle.name = text.trim();
    else if (kind === 'leave') app.leaveCircle(space);
    onclose();
  }

  function askDelete() {
    const name = circle?.name ?? 'this space';
    const key = space;
    onclose();
    ui.confirm = {
      title: 'Delete space',
      text: `Delete ${name} for everyone? Its list of channels and members is removed. This cannot be undone.`,
      confirm: 'Delete space',
      onconfirm: () => void app.deleteCircle(key)
    };
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
      copied = true;
    } catch {
      ui.say('Could not copy the link.', true);
    }
  }

  const levels: { v: NotificationLevel; label: string }[] = [
    { v: 'all', label: 'All messages' },
    { v: 'mentions', label: 'Only mentions' },
    { v: 'nothing', label: 'Nothing' }
  ];
</script>

<Modal title={titles[kind]} {onclose}>
  <form
    id="circle-dialog"
    onsubmit={(e) => {
      e.preventDefault();
      save();
    }}
  >
    {#if kind === 'invite'}
      <p class="hint">Pick people to invite to {circle?.name}. Each one gets a message with the join link.</p>
      <InviteList bind:picked />
      <p class="hint or">Or copy the link and send it yourself.</p>
      <div class="copy">
        <code class="mono">{link}</code>
        <button type="button" class="btn" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
      </div>
    {:else if kind === 'notifications'}
      <fieldset>
        <legend class="field-label">Tell me about</legend>
        {#each levels as l (l.v)}
          <label class="radio"><input type="radio" name="level" value={l.v} bind:group={level} /> {l.label}</label>
        {/each}
      </fieldset>
      <label class="radio"><input type="checkbox" bind:checked={mute} /> Mute this space</label>
    {:else if kind === 'leave'}
      <p class="hint">
        You will leave {circle?.name} and lose its channels. You can join again if it is public.
      </p>
    {:else}
      <div class="field">
        <label for="dlg-text">
          {kind === 'create-channel' ? 'Channel name' : kind === 'nickname' ? 'Nickname' : 'Space name'}
        </label>
        <input
          id="dlg-text"
          class="input"
          bind:value={text}
          autocomplete="off"
          readonly={live && kind === 'settings' && !owner}
        />
        {#if live && kind === 'settings' && !owner}
          <span class="hint">Only the owner of a space can change it.</span>
        {/if}
      </div>
      {#if live && kind === 'settings' && owner}
        <div class="field">
          <label for="dlg-description">Description</label>
          <textarea id="dlg-description" class="input" rows="3" bind:value={description}></textarea>
        </div>
        <div class="field">
          <span class="field-label">Images</span>
          <div class="req">
            <label class="btn" for="dlg-avatar">Change avatar</label>
            <input
              id="dlg-avatar"
              class="sr-only"
              type="file"
              accept="image/*"
              disabled={busy}
              onchange={(e) => void pickImage(e, false)}
            />
            <label class="btn" for="dlg-banner">Change banner</label>
            <input
              id="dlg-banner"
              class="sr-only"
              type="file"
              accept="image/*"
              disabled={busy}
              onchange={(e) => void pickImage(e, true)}
            />
          </div>
        </div>
        <div class="field">
          <span class="field-label">Members</span>
          {#each members as m (m.jid)}
            <div class="req">
              <span class="mono">{m.jid}</span>
              <span class="hint">{affiliationLabel[m.affiliation] ?? m.affiliation}</span>
              {#if m.affiliation !== 'owner'}
                <button
                  type="button"
                  class="btn btn-ghost"
                  aria-label="{m.affiliation === 'outcast' ? 'Unban' : 'Remove'} {m.jid}"
                  onclick={() => void affiliate(m.jid, false)}
                >
                  {m.affiliation === 'outcast' ? 'Unban' : 'Remove'}
                </button>
                {#if m.affiliation !== 'outcast'}
                  <button
                    type="button"
                    class="btn btn-ghost"
                    aria-label="Ban {m.jid}"
                    onclick={() => void affiliate(m.jid, true)}>Ban</button
                  >
                {/if}
              {/if}
            </div>
          {/each}
        </div>
        {#if settingsError}<span class="hint err" role="alert">{settingsError}</span>{/if}
      {/if}
      {#if live && kind === 'settings'}
        <div class="field">
          <span class="field-label">Join requests</span>
          {#each requests as r (r.jid)}
            <div class="req">
              <span class="mono">{r.jid}</span>
              <button
                type="button"
                class="btn"
                aria-label="Approve {r.jid}"
                onclick={() => void answer(r.jid, true)}>Approve</button
              >
              <button
                type="button"
                class="btn btn-ghost"
                aria-label="Deny {r.jid}"
                onclick={() => void answer(r.jid, false)}>Deny</button
              >
            </div>
          {/each}
          {#if requestsNote}<span class="hint">{requestsNote}</span>{/if}
        </div>
      {/if}
      {#if owner}
        <div class="field">
          <span class="field-label">Channels</span>
          {#each channelsHere as c (c.jid)}
            <div class="req">
              <span class="mono">#{c.name}</span>
              <button
                type="button"
                class="btn btn-ghost"
                aria-label="Remove #{c.name} from space"
                onclick={() => void app.removeChannelFromCircle(space, c.jid)}
              >
                Remove from space
              </button>
            </div>
          {:else}
            <span class="hint">This space has no channels.</span>
          {/each}
        </div>
        <div class="field">
          <span class="field-label">Danger zone</span>
          <div>
            <button type="button" class="btn btn-danger" onclick={askDelete}>Delete space</button>
          </div>
        </div>
      {/if}
    {/if}
  </form>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    {#if kind === 'invite'}
      <button class="btn btn-primary" type="submit" form="circle-dialog" disabled={!picked.length || sending}>
        {picked.length > 1 ? `Send ${picked.length} invites` : 'Send invite'}
      </button>
    {:else}
      <button
        class="btn"
        class:btn-danger={kind === 'leave'}
        class:btn-primary={kind !== 'leave'}
        type="submit"
        form="circle-dialog"
        disabled={busy || ((kind === 'create-channel' || kind === 'nickname') && !text.trim())}
      >
        {kind === 'leave' ? 'Leave space' : kind === 'create-channel' ? 'Create channel' : 'Save'}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .hint {
    margin: 0;
    color: var(--ink-muted);
  }
  .req {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .req .mono {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .or {
    margin-top: var(--space-4);
  }
  .err {
    color: var(--danger);
  }
  .copy {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-3);
    background: var(--surface-100);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .radio {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  input[type='radio'],
  input[type='checkbox'] {
    accent-color: var(--brand);
    width: 16px;
    height: 16px;
  }
</style>
