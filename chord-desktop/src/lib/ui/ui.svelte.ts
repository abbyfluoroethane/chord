// Small shell state: which panels and dialogs are open, and local notes.
import { live } from './bridge';
import { settings } from './local';
import type { DialogKind } from './CircleDialog.svelte';
import type { Placement } from './Popover.svelte';
import type { SettingsPage, TimelineItem } from './types';

const MEMBERS_KEY = 'chord.membersOpen';
const NOTES_KEY = 'chord.notes';

export interface PopoutState {
  address: string;
  name: string | null;
  anchor: HTMLElement;
  placement: Placement;
  focusNote: boolean;
}

export interface PersonMenuState {
  address: string;
  name: string | null;
  anchor: HTMLElement;
  placement: Placement;
}

/** A question before a step that cannot be undone. */
export interface ConfirmState {
  title: string;
  text: string;
  confirm: string;
  onconfirm: () => void;
}

/** A room that asks for a password. `resolve` gets the text, or null when the user cancels. */
export interface PasswordAsk {
  room: string;
  /** True when we tried a password already and the room refused it. */
  again: boolean;
  resolve: (password: string | null) => void;
}

/** An anchor for a menu that opens at the pointer. Popover only needs two methods. */
export function pointAnchor(x: number, y: number): HTMLElement {
  return {
    getBoundingClientRect: () => new DOMRect(x, y - 4, 0, 0),
    contains: () => false
  } as unknown as HTMLElement;
}

class UiState {
  membersOpen = $state(true);
  switcherOpen = $state(false);
  shortcutsOpen = $state(false);
  addCircleOpen = $state(false);
  settingsOpen = $state(false);
  settingsPage = $state<SettingsPage>('account');
  logoutOpen = $state(false);
  nicknameOpen = $state(false);
  /** The contact that the rename dialog edits. */
  renaming = $state<{ address: string; name: string } | null>(null);
  /** A space dialog (invite, settings, leave) opened from a menu. */
  circleDialog = $state<{ kind: DialogKind; space: string } | null>(null);
  confirm = $state<ConfirmState | null>(null);
  /** The question for the password of a room. */
  passwordAsk = $state<PasswordAsk | null>(null);
  /** The message in the forward dialog. */
  forwarding = $state<TimelineItem | null>(null);
  /** Popovers and dialogs that are open now. Esc marks read only at 0. */
  overlays = $state(0);

  popout = $state<PopoutState | null>(null);
  personMenu = $state<PersonMenuState | null>(null);
  /** Address of the full profile that is open. */
  profile = $state<string | null>(null);

  /** Notes about people, per address. Local to this device. */
  notes = $state<Record<string, string>>({});
  toast = $state<string | null>(null);
  private toastTimer: ReturnType<typeof setTimeout> | undefined;

  load() {
    try {
      const v = localStorage.getItem(MEMBERS_KEY);
      if (v !== null) this.membersOpen = v === '1';
      // Notes about people live in the local settings inside the app.
      if (live) this.notes = settings.get<Record<string, string>>('notes') ?? {};
      else {
        const n = localStorage.getItem(NOTES_KEY);
        if (n) this.notes = JSON.parse(n) as Record<string, string>;
      }
    } catch {
      /* ignore */
    }
  }

  toggleMembers() {
    this.membersOpen = !this.membersOpen;
    try {
      localStorage.setItem(MEMBERS_KEY, this.membersOpen ? '1' : '0');
    } catch {
      /* ignore */
    }
  }

  // --- people ------------------------------------------------------

  /** Open the small profile card. A second click on the same anchor closes it. */
  openPopout(
    address: string,
    anchor: HTMLElement,
    name: string | null = null,
    placement: Placement = 'left-start',
    focusNote = false
  ) {
    this.personMenu = null;
    if (this.popout?.anchor === anchor && !focusNote) {
      this.popout = null;
      return;
    }
    this.popout = { address, name, anchor, placement, focusNote };
  }

  /** Open the person menu. Give a button for the "more" buttons, or use the pointer. */
  openPersonMenu(e: MouseEvent | KeyboardEvent, address: string, name: string | null = null) {
    e.preventDefault();
    e.stopPropagation();
    this.popout = null;
    const target = e.currentTarget as HTMLElement | null;
    if (e.type === 'contextmenu' && e instanceof MouseEvent) {
      this.personMenu = {
        address,
        name,
        anchor: pointAnchor(e.clientX, e.clientY),
        placement: 'bottom-start'
      };
    } else if (target) {
      this.personMenu = { address, name, anchor: target, placement: 'bottom-start' };
    }
  }

  openProfile(address: string) {
    this.popout = null;
    this.personMenu = null;
    this.profile = address;
  }

  closePeople() {
    this.popout = null;
    this.personMenu = null;
  }

  noteFor(address: string): string {
    return this.notes[address] ?? '';
  }

  setNote(address: string, text: string) {
    if (text.trim()) this.notes[address] = text;
    else delete this.notes[address];
    if (live) {
      settings.set('notes', $state.snapshot(this.notes));
      return;
    }
    try {
      localStorage.setItem(NOTES_KEY, JSON.stringify(this.notes));
    } catch {
      /* ignore */
    }
  }

  // --- settings and notices ---------------------------------------

  openSettings(page: SettingsPage = this.settingsPage) {
    this.closePeople();
    this.settingsPage = page;
    this.settingsOpen = true;
  }

  /** Ask the user for the password of a room. Resolves with null when the user cancels. */
  askPassword(room: string, again: boolean): Promise<string | null> {
    return new Promise((resolve) => {
      this.passwordAsk?.resolve(null);
      this.passwordAsk = { room, again, resolve };
    });
  }

  say(text: string) {
    this.toast = text;
    clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toast = null), 2400);
  }
}

export const ui = new UiState();
