// The app-native context menu. Any component calls `contextMenu.open(event, items)`.
// The host (ContextMenuHost.svelte) draws the menu. The webview menu never shows, except
// in a text field, where it stays for paste and spelling (see `installContextGuard`).
import type { MenuItem } from './Menu.svelte';
import type { Placement } from './Popover.svelte';
import { pointAnchor } from './ui.svelte';

/** The quick reaction row above the items. */
export interface QuickReactions {
  emojis: string[];
  onpick: (emoji: string) => void;
}

export interface ContextMenuState {
  anchor: HTMLElement;
  items: MenuItem[];
  label: string;
  placement: Placement;
  quick?: QuickReactions;
}

export interface OpenOptions {
  /** Read by screen readers. */
  label?: string;
  quick?: QuickReactions;
}

/** True for the keys that open a context menu from the keyboard. */
export function isMenuKey(e: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'ctrlKey' | 'altKey' | 'metaKey'>): boolean {
  if (e.ctrlKey || e.altKey || e.metaKey) return false;
  return e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10');
}

/** An element where the webview menu stays: a text field, or an editable region. */
export function allowsNativeMenu(target: unknown): boolean {
  const node = target as { closest?: (s: string) => unknown; parentElement?: unknown } | null;
  const el = node && typeof node.closest === 'function' ? node : (node?.parentElement as typeof node);
  if (!el || typeof el.closest !== 'function') return false;
  return !!el.closest('input, textarea, [contenteditable]:not([contenteditable="false"])');
}

/**
 * Stop the webview menu everywhere but in a text field. Call once at the root. It
 * returns a function that takes the handler away again.
 */
export function installContextGuard(): () => void {
  const guard = (e: MouseEvent) => {
    if (!allowsNativeMenu(e.target)) e.preventDefault();
  };
  document.addEventListener('contextmenu', guard);
  return () => document.removeEventListener('contextmenu', guard);
}

class ContextMenuStore {
  state = $state.raw<ContextMenuState | null>(null);
  private openedAt = 0;

  get isOpen(): boolean {
    return this.state !== null;
  }

  /**
   * Open a menu for an event. A `contextmenu` event puts it at the pointer. A key press
   * puts it at the element that has focus. A click (the "more" button) puts it under the
   * button, and a second click closes it. With no rows the event is still handled, so
   * the webview menu does not show.
   */
  open(e: Event, items: MenuItem[], options: OpenOptions = {}) {
    e.preventDefault();
    e.stopPropagation();
    // A key press can send a contextmenu event too. Take the first of the two.
    const now = performance.now();
    if (e.type === 'contextmenu' && now - this.openedAt < 120) return;
    if (!items.length && !options.quick) return;
    const target = e.currentTarget as HTMLElement | null;
    let anchor: HTMLElement;
    let placement: Placement = 'bottom-start';
    if (e.type === 'contextmenu' && e instanceof MouseEvent) {
      anchor = pointAnchor(e.clientX, e.clientY);
    } else if (e.type === 'keydown' && target) {
      const r = target.getBoundingClientRect();
      anchor = pointAnchor(r.left + Math.min(72, r.width / 2), r.top + Math.min(r.height, 44));
    } else if (target) {
      if (this.state?.anchor === target) {
        this.close();
        return;
      }
      anchor = target;
      placement = 'bottom-end';
    } else return;
    this.openedAt = now;
    this.state = {
      anchor,
      items,
      placement,
      label: options.label ?? 'Menu',
      quick: options.quick
    };
  }

  close() {
    this.state = null;
  }
}

export const contextMenu = new ContextMenuStore();
