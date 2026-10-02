// Small shell state: which panels and dialogs are open, and local notes.
import { live } from './bridge';
import { settings } from './local';
import type { DialogKind } from './CircleDialog.svelte';
import type { Placement } from './Popover.svelte';
import type { DataForm } from '$lib/chord/types';
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

/** A join that would make a new room. `resolve` gets true when the user says to make it. */
export interface CreateRoomAsk {
  room: string;
  resolve: (create: boolean) => void;
}

/** A room that holds our join until we solve a CAPTCHA (XEP-0158). */
export interface CaptchaAsk {
  room: string;
  form: DataForm;
}

/** An anchor for a menu that opens at the pointer. Popover only needs two methods. */
export function pointAnchor(x: number, y: number): HTMLElement {
  return {
    getBoundingClientRect: () => new DOMRect(x, y - 4, 0, 0),
    contains: () => false
  } as unknown as HTMLElement;
}

/** Actions that a shortcut asks of a component that owns the thing. */
export type UiRequest = 'emoji' | 'gif' | 'upload' | 'search' | 'pins' | 'composer';

class UiState {
  private requestHandlers = new Map<UiRequest, () => void>();

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
  /** The question before a join makes a new room. */
  createRoomAsk = $state<CreateRoomAsk | null>(null);
  /** The CAPTCHA that a room asks for. */
  captchaAsk = $state<CaptchaAsk | null>(null);
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
  /** True if the toast reports a failure. A screen reader reads it out at once. */
  toastIsError = $state(false);
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

  /**
   * A component calls this to answer a request. It returns a function that removes the
   * handler. Use it as the return value of an effect.
   */
  onRequest(kind: UiRequest, handler: () => void): () => void {
    this.requestHandlers.set(kind, handler);
    return () => {
      if (this.requestHandlers.get(kind) === handler) this.requestHandlers.delete(kind);
    };
  }

  /** Ask the component that owns `kind` to act. Returns false when no component listens. */
  request(kind: UiRequest): boolean {
    const handler = this.requestHandlers.get(kind);
    if (!handler) return false;
    handler();
    return true;
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

  /** Ask before a join makes a new room. Resolves with false when the user cancels. */
  askCreateRoom(room: string): Promise<boolean> {
    return new Promise((resolve) => {
      this.createRoomAsk?.resolve(false);
      this.createRoomAsk = { room, resolve };
    });
  }

  say(text: string, error = false) {
    this.toast = text;
    this.toastIsError = error;
    clearTimeout(this.toastTimer);
    // An error, or a long text, stays longer: the reader needs time to read it.
    const ms = error ? 6000 : Math.min(6000, 2400 + text.length * 30);
    this.toastTimer = setTimeout(() => (this.toast = null), ms);
  }
}

export const ui = new UiState();
