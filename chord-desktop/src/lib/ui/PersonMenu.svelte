<script lang="ts">
  // The context menu for a person. Same items from every place that opens it.
  import Ban from 'lucide-svelte/icons/ban';
  import CheckCheck from 'lucide-svelte/icons/check-check';
  import Bell from 'lucide-svelte/icons/bell';
  import Shield from 'lucide-svelte/icons/shield';
  import Copy from 'lucide-svelte/icons/copy';
  import LogOut from 'lucide-svelte/icons/log-out';
  import Mic from 'lucide-svelte/icons/mic';
  import MicOff from 'lucide-svelte/icons/mic-off';
  import MessageSquare from 'lucide-svelte/icons/message-square';
  import Pencil from 'lucide-svelte/icons/pencil';
  import StickyNote from 'lucide-svelte/icons/sticky-note';
  import User from 'lucide-svelte/icons/user';
  import UserMinus from 'lucide-svelte/icons/user-minus';
  import UserPlus from 'lucide-svelte/icons/user-plus';
  import Users from 'lucide-svelte/icons/users';
  import Menu, { type MenuItem } from './Menu.svelte';
  import { app, HOME } from './app.svelte';
  import { live } from './bridge';
  import { contactsStore } from './contacts.svelte';
  import { canModerateMember } from './rooms';
  import { spaceKey, type NotificationLevel, type SpaceItem } from './types';
  import { ui, type PersonMenuState } from './ui.svelte';

  let { state: s }: { state: PersonMenuState } = $props();

  const p = $derived(contactsStore.person(s.address, s.name));
  const circles = $derived(contactsStore.circlesYouAdmin());

  function copy() {
    void navigator.clipboard?.writeText(p.address);
    ui.say('Address copied.');
  }

  // Maps to api.addSpaceMember(service, node, jid) and api.inviteToRoom(room, jid).
  async function invite(c: SpaceItem) {
    if (await app.inviteToCircle(spaceKey(c), p.address)) ui.say(`Invited ${p.name} to ${c.name}.`);
  }

  // A DM has its own notification level. Maps to api.setNotificationLevel(peer, level).
  const dm = $derived(app.channels.find((c) => c.kind === 'dm' && c.jid === p.address));
  const levels: { v: NotificationLevel; label: string }[] = [
    { v: 'all', label: 'All messages' },
    { v: 'mentions', label: 'Only mentions' },
    { v: 'nothing', label: 'Nothing' }
  ];
  // The role of a member in the open room. Maps to api.setRoomAffiliation(room, jid, affiliation).
  const canSetRole = $derived(
    live && app.isRoomAdmin && !p.isMe && p.affiliation !== null && /^[^/]+@[^/]+$/.test(p.address)
  );
  const roles = [
    { v: 'admin', label: 'Admin' },
    { v: 'member', label: 'Member' },
    { v: 'none', label: 'Guest' }
  ] as const;

  // Kick and mute are role changes of an occupant. Maps to api.setRoomRole(room, nick, role).
  const target = $derived(app.membersHere.find((m) => m.id === p.address));
  const mine = $derived(app.membersHere.find((m) => m.id === app.me.address));
  const canKick = $derived(live && !!target?.nick && canModerateMember(mine, target));
  const muted = $derived(target?.role === 'Visitor');

  const items = $derived.by<MenuItem[]>(() => {
    const out: MenuItem[] = [
      { label: 'Profile', icon: User, onselect: () => ui.openProfile(p.address) }
    ];
    if (dm && (dm.unread > 0 || dm.mentions > 0)) {
      out.push({ label: 'Mark as read', icon: CheckCheck, onselect: () => app.markRead(dm.jid) });
    }
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
        label: 'Invite to space',
        icon: Users,
        submenu: circles.map((c) => ({
          label: c.name,
          onselect: () => void invite(c)
        })),
        onselect: () => {}
      });
    }
    if (!p.isMe && p.isContact) {
      out.push({
        label: 'Rename contact',
        icon: Pencil,
        onselect: () => (ui.renaming = { address: p.address, name: p.name })
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
    if (dm) {
      out.push({
        label: 'Notifications',
        icon: Bell,
        submenu: levels.map((l) => ({
          label: l.label,
          checked: app.levelOf(dm.jid) === l.v,
          onselect: () => void app.setLevel(dm.jid, l.v)
        })),
        onselect: () => {}
      });
    }
    if (canSetRole) {
      out.push({
        label: 'Role in this channel',
        icon: Shield,
        separator: true,
        submenu: [
          ...roles.map((r) => ({
            label: r.label,
            checked: p.affiliation === r.v,
            onselect: () => void app.setAffiliation(p.address, r.v)
          })),
          {
            label: 'Ban from this channel',
            danger: true,
            separator: true,
            onselect: () => void app.setAffiliation(p.address, 'outcast')
          }
        ],
        onselect: () => {}
      });
    }
    if (canKick) {
      out.push(
        {
          label: muted ? 'Unmute' : 'Mute',
          icon: muted ? Mic : MicOff,
          separator: true,
          onselect: () => void app.setRole(p.address, muted ? 'participant' : 'visitor')
        },
        {
          label: 'Kick from this channel',
          icon: LogOut,
          danger: true,
          onselect: () =>
            (ui.confirm = {
              title: `Kick ${p.name}?`,
              text: `${p.name} leaves the channel now. They can come back if the channel is open to them.`,
              confirm: 'Kick',
              onconfirm: () => void app.setRole(p.address, 'none')
            })
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
