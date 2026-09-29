// Theme choice: follow the system, or force dark or light through data-theme.

export type ThemeChoice = 'system' | 'dark' | 'light';

const KEY = 'chord.theme';

class ThemeStore {
  choice = $state<ThemeChoice>('system');

  load() {
    try {
      const v = localStorage.getItem(KEY);
      if (v === 'dark' || v === 'light' || v === 'system') this.choice = v;
    } catch {
      /* storage blocked, keep the default */
    }
    this.apply();
  }

  set(choice: ThemeChoice) {
    this.choice = choice;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      /* ignore */
    }
    this.apply();
  }

  private apply() {
    const root = document.documentElement;
    if (this.choice === 'system') root.removeAttribute('data-theme');
    else root.setAttribute('data-theme', this.choice);
  }
}

export const theme = new ThemeStore();
