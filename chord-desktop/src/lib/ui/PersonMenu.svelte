<script lang="ts">
  // The context menu for a person. Same items from every place that opens it.
  import Ban from 'lucide-svelte/icons/ban';
  import Copy from 'lucide-svelte/icons/copy';
  import MessageSquare from 'lucide-svelte/icons/message-square';
  import Pencil from 'lucide-svelte/icons/pencil';
  import StickyNote from 'lucide-svelte/icons/sticky-note';
  import User from 'lucide-svelte/icons/user';
  import UserMinus from 'lucide-svelte/icons/user-minus';
  import UserPlus from 'lucide-svelte/icons/user-plus';
  import Users from 'lucide-svelte/icons/users';
  import Menu, { type MenuItem } from './Menu.svelte';
  import { app, HOME } from './app.svelte';
  import { contactsStore } from './contacts.svelte';
  import { ui, type PersonMenuState } from './ui.svelte';

  let { state: s }: { state: PersonMenuState } = $props();

  const p = $derived(contactsStore.person(s.address, s.name));
  const circles = $derived(contactsStore.circlesYouAdmin());

  function copy() {
    void navigator.clipboard?.writeText(p.address);
    ui.say('Address copied.');
  }

  function invite(name: string) {
    // TODO: await api.inviteToRoom(room, address)
    ui.say(`Invited ${p.name} to ${name}.`);
  }

  const items = $derived.by<MenuItem[]>(() => {
    const out: MenuItem[] = [
      { label: 'Profile', icon: User, onselect: () => ui.openProfile(p.address) }
    ];
    if (!p.isMe) {
      out.push({
        label: 'Message',
        icon: MessageSquare,
        onselect: () => contactsStore.message(p.address, p.name)
      });
    }
    out.push({
      label: 'Add note',
      icon: StickyNote,
      onselect: () => {
        const a = s.anchor;
        ui.openPopout(p.address, a, p.name, s.placement === 'bottom-start' ? 'right-start' : s.placement, true);
      }
    });
    if (!p.isMe && circles.length) {
      out.push({
        label: 'Invite to circle',
        icon: Users,
        submenu: circles.map((c) => ({
          label: c.name,
          onselect: () => invite(c.name)
        })),
        onselect: () => {}
      });
    }
    if (!p.isMe) {
      out.push(
        p.isContact
          ? {
              label: 'Remove contact',
              icon: UserMinus,
              separator: true,
              onselect: () => contactsStore.remove(p.address)
            }
          : {
              label: 'Add contact',
              icon: UserPlus,
              separator: true,
              disabled: p.isBlocked,
              onselect: () => {
                const r = contactsStore.add(p.address);
                ui.say(r.ok ? r.message : r.error);
              }
            }
      );
    }
    if (p.isMe && app.selectedSpace !== HOME) {
      out.push({
        label: 'Change nickname',
        icon: Pencil,
        onselect: () => (ui.nicknameOpen = true)
      });
    }
    if (!p.isMe) {
      out.push(
        p.isBlocked
          ? { label: 'Unblock', icon: Ban, danger: true, onselect: () => contactsStore.unblock(p.address) }
          : { label: 'Block', icon: Ban, danger: true, onselect: () => contactsStore.block(p.address) }
      );
    }
    out.push({ label: 'Copy address', icon: Copy, separator: true, onselect: copy });
    return out;
  });
</script>

<Menu
  anchor={s.anchor}
  {items}
  placement={s.placement}
  label="Menu for {p.name}"
  onclose={() => (ui.personMenu = null)}
/>
