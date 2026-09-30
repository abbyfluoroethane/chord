<script lang="ts">
  // The question before any xmpp: link does anything. A link from a message or from the OS
  // opens this dialog. Nothing happens until the user clicks the main button.
  import Modal from './Modal.svelte';
  import { contactsStore } from './contacts.svelte';
  import { xmppLinks } from './xmpplinks.svelte';
  import type { KnownXmppLink } from './xmppuri';

  let { link, onclose }: { link: KnownXmppLink; onclose: () => void } = $props();

  const info = $derived(link.kind === 'space' || link.kind === 'room' ? xmppLinks.get(link) : undefined);
  const invalid = $derived(info === null);
  const joined = $derived(xmppLinks.joined(link));
  let busy = $state(false);

  const address = $derived(
    link.kind === 'space' ? `${link.service} / ${link.node}` : link.kind === 'room' ? link.jid : link.jid
  );

  const title = $derived.by(() => {
    switch (link.kind) {
      case 'space':
        return `Join ${info?.kind === 'space' ? info.name : link.node}?`;
      case 'room':
        return `Join #${info?.kind === 'room' ? info.name : link.jid.split('@')[0]}?`;
      case 'chat':
        return `Message ${link.jid}?`;
      case 'contact':
        return contactsStore.isContact(link.jid) ? `Message ${link.jid}?` : `Add ${link.jid} to contacts?`;
    }
  });

  const confirm = $derived(
    link.kind === 'space' || link.kind === 'room'
      ? joined
        ? 'Open'
        : 'Join'
      : link.kind === 'contact' && !contactsStore.isContact(link.jid)
        ? 'Add contact'
        : 'Message'
  );

  const note = $derived.by(() => {
    if (invalid) return 'This invite is not valid. The server does not know it, or it refuses to tell.';
    if (link.kind === 'space' || link.kind === 'room') {
      return 'Join only if you trust the person who sent this link.';
    }
    if (link.kind === 'contact') return 'The person can see that you asked, and can accept or ignore it.';
    return 'The chat opens with an empty message box. Nothing is sent yet.';
  });

  async function confirmed() {
    if (busy) return;
    busy = true;
    const ok = await xmppLinks.act(link, info);
    busy = false;
    if (ok) onclose();
  }
</script>

<Modal {title} {onclose}>
  <p class="address">{address}</p>
  {#if info?.kind === 'space' || info?.kind === 'room'}
    {@const text = info.kind === 'space' ? info.description : info.subject}
    {#if text}<p class="about">{text}</p>{/if}
  {/if}
  <p class="note" class:bad={invalid}>{note}</p>

  {#snippet footer()}
    <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    <button class="btn btn-primary" disabled={invalid || busy} onclick={confirmed}>{confirm}</button>
  {/snippet}
</Modal>

<style>
  .address {
    margin: 0;
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    font-size: 13px;
    color: var(--ink-muted);
  }
  .about {
    margin: var(--space-3) 0 0;
  }
  .note {
    margin: var(--space-3) 0 0;
    color: var(--ink-muted);
  }
  .note.bad {
    color: var(--danger);
  }
</style>
