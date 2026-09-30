// The rows of the context menus, in one place. Each builder returns plain data. The
// components open the result with `contextMenu.open(event, items)`.
import Bell from 'lucide-svelte/icons/bell';
import BellOff from 'lucide-svelte/icons/bell-off';
import CheckCheck from 'lucide-svelte/icons/check-check';
import Copy from 'lucide-svelte/icons/copy';
import Download from 'lucide-svelte/icons/download';
import ExternalLink from 'lucide-svelte/icons/external-link';
import Forward from 'lucide-svelte/icons/forward';
import Hash from 'lucide-svelte/icons/hash';
import Image from 'lucide-svelte/icons/image';
import Link from 'lucide-svelte/icons/link';
import LogOut from 'lucide-svelte/icons/log-out';
import Mail from 'lucide-svelte/icons/mail';
import Pencil from 'lucide-svelte/icons/pencil';
import Reply from 'lucide-svelte/icons/reply';
import Settings from 'lucide-svelte/icons/settings';
import ShieldX from 'lucide-svelte/icons/shield-x';
import SmilePlus from 'lucide-svelte/icons/smile-plus';
import TextSelect from 'lucide-svelte/icons/text-select';
import Trash from 'lucide-svelte/icons/trash-2';
import UserPlus from 'lucide-svelte/icons/user-plus';
import { app } from './app.svelte';
import { openLink, saveImage, viewImage } from './attachments';
import { live } from './bridge';
import { copyText } from './clipboard';
import type { QuickReactions } from './contextmenu.svelte';
import type { MenuItem } from './Menu.svelte';
import { channelLink } from './messagelink';
import type { Attachment, ChannelItem, NotificationLevel, TimelineItem } from './types';
import { ui } from './ui.svelte';

/** Join groups of rows. A divider goes above the first row of every group but the first. */
export function sections(...groups: MenuItem[][]): MenuItem[] {
  const out: MenuItem[] = [];
  for (const group of groups) {
    group.forEach((item, i) => out.push(i === 0 && out.length ? { ...item, separator: true } : item));
  }
  return out;
}

const LEVELS: { v: NotificationLevel; label: string }[] = [
  { v: 'all', label: 'All messages' },
  { v: 'mentions', label: 'Only mentions' },
  { v: 'nothing', label: 'Nothing' }
];

const MUTES: { label: string; ms: number | null }[] = [
  { label: 'For 15 minutes', ms: 15 * 60_000 },
  { label: 'For 1 hour', ms: 60 * 60_000 },
  { label: 'For 8 hours', ms: 8 * 60 * 60_000 },
  { label: 'For 24 hours', ms: 24 * 60 * 60_000 },
  { label: 'Until I turn it back on', ms: null }
];

// --- messages ---------------------------------------------------------

/** What the pointer was on when the menu opened, inside the message. */
export interface MessageTarget {
  link?: string | null;
  image?: Attachment | null;
  selection?: string;
}

/** The id that another person can use: the id from the server or the room, if we have it. */
export function messageId(item: TimelineItem): string {
  return item.stanzaId || item.originId || item.id;
}

export function messageMenu(
  item: TimelineItem,
  target: MessageTarget,
  ondelete: (e: MouseEvent | KeyboardEvent) => void
): { items: MenuItem[]; quick?: QuickReactions } {
  const gone = item.retracted;
  const channel = app.channel;

  const here: MenuItem[] = [];
  if (target.image) {
    const image = target.image;
    here.push(
      { label: 'Open image', icon: Image, onselect: () => viewImage(image) },
      {
        label: 'Copy image link',
        icon: Link,
        onselect: () => void copyText(image.url, 'Image link copied.')
      }
    );
    // Saving needs the system dialog, so it works inside the app only.
    if (live) here.push({ label: 'Save image', icon: Download, onselect: () => void saveImage(image) });
  }
  if (target.link) {
    const link = target.link;
    here.push(
      { label: 'Open link', icon: ExternalLink, onselect: () => void openLink(link) },
      { label: 'Copy link', icon: Link, onselect: () => void copyText(link, 'Link copied.') }
    );
  }
  if (target.selection) {
    const text = target.selection;
    here.push({ label: 'Copy selection', icon: TextSelect, onselect: () => void copyText(text) });
  }

  const react: MenuItem[] = gone
    ? []
    : [
        {
          label: 'Add reaction',
          icon: SmilePlus,
          picker: (emoji) => app.toggleReaction(item.id, emoji)
        }
      ];

  const act: MenuItem[] = [];
  if (!gone) {
    if (app.canEdit(item)) {
      act.push({ label: 'Edit message', icon: Pencil, onselect: () => (app.editingId = item.id) });
    }
    act.push({ label: 'Reply', icon: Reply, onselect: () => app.startReply(item) });
    if (item.body.trim() || item.attachment) {
      act.push({ label: 'Forward', icon: Forward, onselect: () => (ui.forwarding = item) });
    }
  }

  const more: MenuItem[] = [
    { label: 'Mark unread', icon: Mail, onselect: () => app.markUnread(item) }
  ];
  if (!gone && item.body) {
    more.push({ label: 'Copy text', icon: Copy, onselect: () => void copyText(item.body, 'Text copied.') });
  }
  if (channel && !channel.pm) {
    more.push({
      label: channel.kind === 'channel' ? 'Copy channel link' : 'Copy contact link',
      icon: Link,
      onselect: () => void copyText(channelLink(channel.jid, channel.kind), 'Link copied.')
    });
  }

  // Your own messages, or any message when you moderate the room.
  const remove: MenuItem[] = [];
  if (!gone && item.outgoing) {
    remove.push({ label: 'Delete message', icon: Trash, danger: true, onselect: ondelete });
  } else if (!gone && app.canModerate) {
    remove.push({ label: 'Remove message', icon: ShieldX, danger: true, onselect: ondelete });
  }

  const id = messageId(item);
  const ids: MenuItem[] = [
    { label: 'Copy message ID', icon: Hash, onselect: () => void copyText(id, 'ID copied.') }
  ];

  return {
    items: sections(here, react, act, more, remove, ids),
    quick: gone
      ? undefined
      : { emojis: app.quickReactions, onpick: (emoji) => app.toggleReaction(item.id, emoji) }
  };
}

// --- channels and spaces ---------------------------------------------

export function levelSubmenu(jid: string, current: NotificationLevel): MenuItem[] {
  return LEVELS.map((l) => ({
    label: l.label,
    checked: current === l.v,
    onselect: () => void app.setLevel(jid, l.v)
  }));
}

export function channelMenu(c: ChannelItem, extra: MenuItem[] = []): MenuItem[] {
  const dm = c.kind === 'dm';
  const quiet = c.unread === 0 && c.mentions === 0;
  const read: MenuItem[] = [
    {
      label: 'Mark as read',
      icon: CheckCheck,
      disabled: quiet,
      hint: quiet ? 'Up to date' : undefined,
      onselect: () => app.markRead(c.jid)
    }
  ];
  const notify: MenuItem[] = [
    {
      label: 'Notification level',
      icon: Bell,
      submenu: levelSubmenu(c.jid, app.levelOf(c.jid))
    },
    c.muted
      ? {
          label: dm ? 'Unmute conversation' : 'Unmute channel',
          icon: Bell,
          onselect: () => void app.unmute(c.jid)
        }
      : {
          label: dm ? 'Mute conversation' : 'Mute channel',
          icon: BellOff,
          submenu: MUTES.map((m) => ({
            label: m.label,
            onselect: () => void app.muteFor(c.jid, m.ms)
          }))
        }
  ];
  const address: MenuItem[] = dm
    ? []
    : [
        {
          label: 'Copy channel address',
          icon: Copy,
          onselect: () => void copyText(c.jid, 'Address copied.')
        }
      ];
  const leave: MenuItem[] =
    !dm && c.joined
      ? [
          {
            label: 'Leave channel',
            icon: LogOut,
            danger: true,
            onselect: () =>
              (ui.confirm = {
                title: 'Leave channel',
                text: `Leave #${c.name}? You stop getting its messages. You can join again later.`,
                confirm: 'Leave channel',
                onconfirm: () => app.leaveRoom(c.jid)
              })
          }
        ]
      : [];
  return sections(read, notify, extra, address, leave);
}

export function circleMenu(key: string): MenuItem[] {
  const badge = app.spaceBadge(key);
  const quiet = badge.unread === 0 && badge.mentions === 0;
  const dialog = (kind: 'invite' | 'settings' | 'leave') => () =>
    (ui.circleDialog = { kind, space: key });
  return sections(
    [
      {
        label: 'Mark as read',
        icon: CheckCheck,
        disabled: quiet,
        hint: quiet ? 'Up to date' : undefined,
        onselect: () => app.markSpaceRead(key)
      }
    ],
    [
      {
        label: 'Notification level',
        icon: Bell,
        submenu: LEVELS.map((l) => ({
          label: l.label,
          checked: (app.notifyLevel[key] ?? 'all') === l.v,
          onselect: () => void app.setCircleLevel(key, l.v)
        }))
      }
    ],
    [
      { label: 'Invite people', icon: UserPlus, onselect: dialog('invite') },
      { label: 'Space settings', icon: Settings, onselect: dialog('settings') }
    ],
    [{ label: 'Leave space', icon: LogOut, danger: true, onselect: dialog('leave') }]
  );
}

/** Home: the menu of the Home tile. */
export function homeMenu(key: string): MenuItem[] {
  const badge = app.spaceBadge(key);
  const quiet = badge.unread === 0 && badge.mentions === 0;
  return [
    {
      label: 'Mark all as read',
      icon: CheckCheck,
      disabled: quiet,
      hint: quiet ? 'Up to date' : undefined,
      onselect: () => app.markSpaceRead(key)
    }
  ];
}
