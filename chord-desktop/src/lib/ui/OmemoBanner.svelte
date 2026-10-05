<script lang="ts">
  // A thin notice under the top bars. It does not block. "Review" opens the devices dialog
  // of a contact with a new key. After a keys reset, "View" opens Settings, Encryption.
  import ShieldAlert from 'lucide-svelte/icons/shield-alert';
  import X from 'lucide-svelte/icons/x';
  import Icon from './Icon.svelte';
  import { contactsStore } from './contacts.svelte';
  import { trustNeededText } from './omemodata';
  import { omemo } from './omemo.svelte';
  import { ui } from './ui.svelte';

  const first = $derived(omemo.notices[0] ?? null);
</script>

{#if first}
  <div class="banner" role="status">
    <Icon icon={ShieldAlert} size={16} />
    <span>{trustNeededText(contactsStore.person(first.jid).name)}</span>
    <button class="act" onclick={() => omemo.openNotice(first.jid)}>Review</button>
    <button class="close" aria-label="Close" onclick={() => omemo.dismissNotice(first.jid)}>
      <Icon icon={X} size={14} />
    </button>
  </div>
{:else if omemo.keysReset && !ui.settingsOpen}
  <div class="banner" role="status">
    <Icon icon={ShieldAlert} size={16} />
    <span>Your encryption keys are new.</span>
    <button class="act" onclick={() => ui.openSettings('encryption')}>View</button>
    <button class="close" aria-label="Close" onclick={() => (omemo.keysReset = false)}>
      <Icon icon={X} size={14} />
    </button>
  </div>
{/if}

<style>
  .banner {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: 32px;
    flex: none;
    padding: 0 40px;
    background: var(--surface-300);
    border-bottom: 1px solid var(--line);
    color: var(--ink);
    font-size: 13px;
    font-weight: 500;
    animation: arrive var(--dur-arrive) var(--ease-out);
  }
  .act {
    text-decoration: underline;
    font-weight: 600;
  }
  .close {
    position: absolute;
    right: var(--space-2);
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
  }
  .close:hover {
    background: var(--hover);
  }
</style>
