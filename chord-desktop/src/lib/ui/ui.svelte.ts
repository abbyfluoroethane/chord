// Small shell state: which panels and dialogs are open.

const MEMBERS_KEY = 'chord.membersOpen';

class UiState {
  membersOpen = $state(true);
  switcherOpen = $state(false);
  shortcutsOpen = $state(false);
  addCircleOpen = $state(false);
  /** Popovers and dialogs that are open now. Esc marks read only at 0. */
  overlays = $state(0);

  load() {
    try {
      const v = localStorage.getItem(MEMBERS_KEY);
      if (v !== null) this.membersOpen = v === '1';
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
}

export const ui = new UiState();
