<script lang="ts">
  // One profile design for the card, the full profile and the right rail of a 1:1 chat.
  // Top to bottom: banner, avatar, name, address, status, actions, then sections.
  // The variant changes the size of the banner and which sections show. Nothing else.
  import { onMount } from 'svelte';
  import Check from 'lucide-svelte/icons/check';
  import Ellipsis from 'lucide-svelte/icons/ellipsis';
  import Shield from 'lucide-svelte/icons/shield';
  import UserPlus from 'lucide-svelte/icons/user-plus';
  import Avatar from './Avatar.svelte';
  import CircleIcon from './CircleIcon.svelte';
  import CopyAddress from './CopyAddress.svelte';
  import EmojiText from './EmojiText.svelte';
  import Icon from './Icon.svelte';
  import PersonMenu from './PersonMenu.svelte';
  import { app } from './app.svelte';
  import { contactsStore } from './contacts.svelte';
  import { dayLabel, tint } from './format';
  import { bannerColor } from './imagecolor';
  import { affiliationLabel, presenceKind, presenceLabel, spaceKey } from './types';
  import { ui } from './ui.svelte';

  let {
    address,
    name = null,
    variant,
    cut,
    focusNote = false,
    onclose = () => {}
  }: {
    address: string;
    name?: string | null;
    /** The card is a popover. The profile is a dialog. The rail sits at the side of a 1:1 chat. */
    variant: 'card' | 'profile' | 'rail';
    /** The colour of the surface behind the card. The avatar ring takes it. */
    cut: string;
    focusNote?: boolean;
    /** Close the host before an action that leaves it. */
    onclose?: () => void;
  } = $props();

  const p = $derived(contactsStore.person(address, name));
  const kind = $derived(presenceKind(p.online, p.show));
  const circles = $derived(contactsStore.sharedCircles(address));
  const mutual = $derived(contactsStore.sharedContacts(address));
  // A room that hides real addresses gives `room/nick`. It is not an address of a person.
  const hidden = $derived(p.address.includes('/'));
  const full = $derived(variant !== 'card');

  // The banner takes its colour from the profile picture. Until the picture is read,
  // and for a person without one, it uses the colour of the initials.
  let banner = $state<string | null>(null);
  $effect(() => {
    const src = p.avatar;
    banner = null;
    if (!src) return;
    let live = true;
    void bannerColor(src).then((c) => {
      if (live) banner = c;
    });
    return () => {
      live = false;
    };
  });

  let note = $state('');
  let noteEl = $state<HTMLTextAreaElement>();
  let text = $state('');
  let more = $state<HTMLElement | null>(null);

  $effect(() => {
    note = ui.noteFor(address);
  });
  onMount(() => {
    // The popover puts focus on its first control. Move it to the note after that.
    if (focusNote) setTimeout(() => noteEl?.focus(), 0);
  });

  function message() {
    onclose();
    contactsStore.message(p.address, p.name);
  }

  function quick(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    e.preventDefault();
    const body = text.trim();
    message();
    if (body) app.send(body);
  }

  function add() {
    const r = contactsStore.add(p.address);
    ui.say(r.ok ? r.message : r.error, !r.ok);
  }

  function openSpace(key: string) {
    onclose();
    app.selectSpace(key);
  }
</script>

<div class="pc {variant}" style:--cut={cut}>
  <div class="banner" style:background={banner ?? tint(p.name)}></div>
  <div class="avatar">
    <Avatar name={p.name} src={p.avatar} size={80} presence={kind} {cut} />
  </div>

  <div class="id">
    <h3 class="name" class:me={p.isMe}>{p.name}</h3>
    <CopyAddress address={p.address} />
    {#if hidden}<span class="meta">This room hides the real address.</span>{/if}
    {#if p.isBlocked}<span class="chip danger tag">Blocked</span>{/if}
    <p class="status">{#if p.status}<EmojiText text={p.status} />{:else}{presenceLabel[kind]}{/if}</p>
  </div>

  <div class="actions">
    {#if p.isMe}
      <button
        class="btn btn-primary grow"
        onclick={() => {
          onclose();
          ui.openSettings('account');
        }}>Edit profile</button
      >
    {:else}
      {#if variant !== 'rail'}
        <button class="btn btn-primary grow" onclick={message}>Message</button>
      {/if}
      {#if p.isBlocked}
        <button class="btn grow" onclick={() => contactsStore.unblock(p.address)}>Unblock</button>
      {:else if p.isContact}
        <span class="state"><Icon icon={Check} size={16} />Contact</span>
      {:else}
        <button
          class="btn grow"
          disabled={hidden}
          title={hidden ? 'This room hides the real address of the person.' : undefined}
          onclick={add}><Icon icon={UserPlus} size={16} />Add contact</button
        >
      {/if}
    {/if}
    <button
      class="btn more"
      aria-label="More actions"
      aria-haspopup="menu"
      aria-expanded={more !== null}
      onclick={(e) => (more = more ? null : e.currentTarget)}
    >
      <Icon icon={Ellipsis} size={18} />
    </button>
  </div>
  {#if more}
    <PersonMenu
      state={{ address: p.address, name: p.name, anchor: more, placement: 'bottom-end' }}
      inCard
      onclose={() => (more = null)}
    />
  {/if}

  <div class="panel">
    {#if p.affiliation}
      <section>
        <h4 class="head">Roles</h4>
        <div class="chips">
          {#if p.affiliation}<span class="chip">{affiliationLabel(p.affiliation)}</span>{/if}
          {#if p.role}
            <span class="chip">
              {#if p.role === 'Moderator'}<Icon icon={Shield} size={12} />{/if}{p.role}
            </span>
          {/if}
        </div>
      </section>
    {/if}
    {#if p.since}
      <section>
        <h4 class="head">Member since</h4>
        <p class="value">{dayLabel(p.since)}</p>
      </section>
    {/if}
    <section>
      <label class="head" for="note-{variant}-{p.address}">Note</label>
      <textarea
        id="note-{variant}-{p.address}"
        bind:this={noteEl}
        rows="2"
        maxlength="200"
        placeholder="Only you can see this. It stays on this device."
        bind:value={note}
        oninput={() => ui.setNote(p.address, note)}
      ></textarea>
    </section>
  </div>

  {#if full}
    <div class="lists">
      <section>
        <h4 class="head">Shared spaces<span class="count">{circles.length}</span></h4>
        {#if circles.length}
          <ul>
            {#each circles as c (spaceKey(c))}
              <li>
                <button class="line" onclick={() => openSpace(spaceKey(c))}>
                  <span class="cicon"><CircleIcon name={c.name} src={c.avatar} fill /></span>
                  <span class="lname">{c.name}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty">You share no spaces.</p>
        {/if}
      </section>
      <section>
        <h4 class="head">Mutual contacts<span class="count">{mutual.length}</span></h4>
        {#if mutual.length}
          <ul>
            {#each mutual as c (c.address)}
              <li>
                <button class="line" title={c.address} onclick={() => (ui.profile = c.address)}>
                  <Avatar
                    name={c.name}
                    src={c.avatar}
                    size={32}
                    presence={presenceKind(c.online, c.show)}
                    {cut}
                  />
                  <span class="lname">{c.name}</span>
                  <span class="mono meta addr">{c.address}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="empty">You share no contacts.</p>
        {/if}
      </section>
      {#if variant === 'rail'}
        <button class="link" onclick={() => ui.openProfile(p.address)}>View full profile</button>
      {/if}
    </div>
  {:else}
    <div class="lists">
      {#if !p.isMe}
        <input
          class="input"
          placeholder="Message @{p.name}"
          aria-label="Message {p.name}"
          bind:value={text}
          onkeydown={quick}
        />
      {/if}
      <button class="link" onclick={() => ui.openProfile(p.address)}>
        View full profile
        <span class="meta">{circles.length} shared {circles.length === 1 ? 'space' : 'spaces'}</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .pc {
    --pad: var(--space-4);
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-bottom: var(--pad);
    min-width: 0;
  }
  .pc.card {
    width: 300px;
  }
  .banner {
    height: 64px;
    border-bottom: 1px solid var(--line);
    transition: background var(--dur-arrive) var(--ease-out);
  }
  .pc.rail .banner,
  .pc.profile .banner {
    height: 88px;
  }
  @media (prefers-reduced-motion: reduce) {
    .banner {
      transition: none;
    }
  }
  .avatar {
    width: fit-content;
    margin: -44px 0 0 var(--pad);
    padding: 4px;
    border-radius: 50%;
    background: var(--cut);
  }
  .id {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 0 var(--pad);
    min-width: 0;
  }
  .name {
    margin: 0;
    max-width: 100%;
    overflow-wrap: anywhere;
    font-size: 20px;
    line-height: 26px;
    font-weight: 600;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .name.me {
    color: var(--brand-ink);
  }
  .status {
    margin: var(--space-1) 0 0;
    max-width: 100%;
    overflow-wrap: anywhere;
    color: var(--ink);
    font-size: 14px;
    line-height: 20px;
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    padding: 0 var(--pad);
  }
  .grow {
    flex: 1;
    min-width: 0;
    padding: 0 var(--space-3);
    white-space: nowrap;
  }
  .more {
    flex: none;
    width: 36px;
    padding: 0;
  }
  .state {
    display: inline-flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    height: 36px;
    color: var(--ink-muted);
    font-weight: 500;
  }
  .panel,
  .lists {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .panel {
    margin: 0 var(--pad);
    padding: var(--space-3);
    background: var(--surface-100);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .lists {
    padding: 0 var(--pad);
  }
  .panel section {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 16px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .count {
    font-weight: 500;
    letter-spacing: 0;
  }
  .value {
    margin: 0;
    font-size: 14px;
    line-height: 20px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 20px;
    font-weight: 500;
  }
  .tag {
    margin-top: var(--space-1);
  }
  .chip.danger {
    border-color: var(--danger);
    color: var(--danger);
  }
  textarea {
    width: 100%;
    resize: none;
    padding: var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-200);
    font-size: 14px;
    line-height: 20px;
  }
  textarea:focus-visible {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .lists section {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 44px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    text-align: left;
    font-weight: 500;
  }
  .line:hover {
    background: var(--hover);
  }
  .lname {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rail .addr {
    display: none;
  }
  .addr {
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cicon {
    display: block;
    flex: none;
    width: 32px;
    height: 32px;
    overflow: hidden;
    border-radius: var(--radius-circle-icon);
    font-size: 13px;
    font-weight: 600;
  }
  .empty {
    margin: 0;
    padding: 0 var(--space-2);
    color: var(--ink-muted);
    font-size: 14px;
  }
  .link {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0;
    color: var(--accent);
    font-size: 14px;
    font-weight: 500;
    text-align: left;
  }
  .link:hover {
    text-decoration: underline;
  }
  .input {
    width: 100%;
  }
</style>
