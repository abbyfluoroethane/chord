<script lang="ts">
  // The right rail of a 1:1 DM: the profile of the peer, where a room shows its members.
  // The member list button in the header shows and hides it (ui.membersOpen).
  import Avatar from './Avatar.svelte';
  import CopyAddress from './CopyAddress.svelte';
  import EmojiText from './EmojiText.svelte';
  import { contactsStore } from './contacts.svelte';
  import { tint } from './format';
  import { bannerColor } from './imagecolor';
  import { presenceKind, presenceLabel } from './types';
  import { ui } from './ui.svelte';

  let { address, name }: { address: string; name: string } = $props();

  const p = $derived(contactsStore.person(address, name));
  const kind = $derived(presenceKind(p.online, p.show));

  // The banner takes its colour from the profile picture, as in the profile card.
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
  $effect(() => {
    note = ui.noteFor(address);
  });
</script>

<aside class="profile" aria-label="Profile of {p.name}">
  <div class="banner" style:background={banner ?? tint(p.name)}></div>
  <div class="avatar">
    <Avatar name={p.name} src={p.avatar} size={80} presence={kind} cut="var(--surface-200)" />
  </div>
  <div class="body">
    <h2 class="title">{p.name}</h2>
    <CopyAddress address={p.address} />
    <p class="status">{#if p.status}<EmojiText text={p.status} />{:else}{presenceLabel[kind]}{/if}</p>

    <hr />
    <div class="field">
      <label class="field-label" for="dm-note-{p.address}">Note</label>
      <textarea
        id="dm-note-{p.address}"
        rows="2"
        maxlength="200"
        placeholder="Only on this device"
        bind:value={note}
        oninput={() => ui.setNote(p.address, note)}
      ></textarea>
    </div>
    <button class="link" onclick={() => ui.openProfile(p.address)}>View full profile</button>
  </div>
</aside>

<style>
  .profile {
    position: relative;
    width: var(--members-width);
    flex: none;
    overflow-y: auto;
    background: var(--surface-200);
    border-left: 1px solid var(--line);
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .banner {
    height: 120px;
    border-bottom: 1px solid var(--line);
    transition: background var(--dur-arrive) var(--ease-out);
  }
  @media (prefers-reduced-motion: reduce) {
    .banner {
      transition: none;
    }
  }
  .avatar {
    position: absolute;
    top: 76px;
    left: var(--space-4);
    padding: 4px;
    border-radius: 50%;
    background: var(--surface-200);
  }
  .body {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: 52px var(--space-4) var(--space-4);
  }
  .title {
    margin: 0;
    font-size: 20px;
  }
  .status {
    margin: 0;
    color: var(--ink-muted);
    font-size: 14px;
  }
  hr {
    width: 100%;
    margin: var(--space-2) 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
  .field {
    width: 100%;
  }
  textarea {
    width: 100%;
    resize: none;
    padding: var(--space-2);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
    background: var(--surface-100);
    font-size: 14px;
    line-height: 20px;
  }
  textarea:focus-visible {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }
  .link {
    color: var(--accent);
    font-size: 14px;
    font-weight: 500;
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
