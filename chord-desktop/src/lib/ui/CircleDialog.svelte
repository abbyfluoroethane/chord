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
  // createChannel, and leaveSpace.
  import { splitSpaceKey } from './adapt';
  import { failureNote } from './batch';
  import InviteList from './InviteList.svelte';
  import Modal from './Modal.svelte';
  import { app } from './app.svelte';
  import { ui } from './ui.svelte';
  import { live } from './bridge';
  import { spaceInviteLink } from './xmppuri';
  import type { NotificationLevel } from './types';

  let { kind, space, onclose }: { kind: DialogKind; space: string; onclose: () => void } = $props();

  const circle = $derived(app.spaceOf(space));
  let text = $state('');
  let level = $state<NotificationLevel>('all');
  let copied = $state(false);
  let mute = $state(false);
  /** Join requests of the space. Only an owner gets them. */
  let requests = $state<{ jid: string; subid: string | null }[]>([]);
  let requestsNote = $state('');

  // Maps to api.spaceJoinRequests(service, node).
  $effect(() => {
    if (!live || kind !== 'settings') return;
    const { service, node } = splitSpaceKey(space);
    void app
      .call((b) => b.spaceJoinRequests(service, node))
      .then((r) => {
        if (r.ok) {
          requests = r.value;
          requestsNote = r.value.length ? '' : 'Nobody is waiting to join.';
        } else requestsNote = 'Only the owner of a space sees join requests.';
      });
  });

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
    // The bridge cannot rename a space yet, so the name is read-only inside the app.
    else if (kind === 'settings' && circle && text.trim() && !live) circle.name = text.trim();
    else if (kind === 'leave') app.leaveCircle(space);
    onclose();
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
          readonly={live && kind === 'settings'}
        />
        {#if live && kind === 'settings'}
          <span class="hint">Renaming a space comes later.</span>
        {/if}
      </div>
      {#if live && kind === 'settings'}
        <div class="field">
          <span class="field-label">Join requests</span>
          {#each requests as r (r.jid)}
            <div class="req">
              <span class="mono">{r.jid}</span>
              <button type="button" class="btn" onclick={() => void answer(r.jid, true)}>Approve</button>
              <button type="button" class="btn btn-ghost" onclick={() => void answer(r.jid, false)}>Deny</button>
            </div>
          {/each}
          {#if requestsNote}<span class="hint">{requestsNote}</span>{/if}
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
