// What the global shortcuts do. `AppShell` calls `handleKey` for each key press.
// `keymap.ts` says which key means which action. This file runs the action.

import { app, HOME } from './app.svelte';
import { jumpTo } from './jump';
import { findBinding, neighbor, railOrder, type Action } from './keymap';
import { rail } from './rail.svelte';
import { mac } from './shortcuts';
import { spaceKey } from './types';
import { escMarksRead } from './unread';
import { ui } from './ui.svelte';

function isEditable(t: EventTarget | null): boolean {
  const el = t as Partial<HTMLElement> | null;
  if (!el || typeof el.tagName !== 'string') return false;
  const tag = el.tagName.toLowerCase();
  return tag === 'input' || tag === 'textarea' || tag === 'select' || !!el.isContentEditable;
}

function inComposer(t: EventTarget | null): boolean {
  return t instanceof HTMLElement && !!t.closest('.composer');
}

/** A key that a button, a link, or a menu item uses for itself. */
function isControl(t: EventTarget | null): boolean {
  return (
    t instanceof HTMLElement &&
    !!t.closest('button, a, summary, [role="button"], [role="menuitem"], [role="option"], [role="tab"]')
  );
}

const timeline = () => document.querySelector<HTMLElement>('.list[role="log"]');

/** A chat is open and no dialog covers it. */
const chatFree = () =>
  !!app.channel && !app.showContacts && ui.overlays === 0 && !ui.settingsOpen && !ui.switcherOpen;

/** A popover closes with the same key that opened it. One popover may be open. */
const popoverFree = () =>
  !!app.channel &&
  !app.showContacts &&
  ui.overlays <= 1 &&
  !ui.settingsOpen &&
  !ui.switcherOpen &&
  !ui.shortcutsOpen &&
  !ui.confirm &&
  !ui.forwarding;

function stepSpace(dir: 1 | -1) {
  const known = app.spaces.map(spaceKey);
  const order = [HOME, ...railOrder(rail.layout, known)];
  const next = neighbor(order, app.selectedSpace, dir);
  if (next !== null && next !== app.selectedSpace) app.selectSpace(next);
}

/** The message that a reply goes to: the focused one, or else the last one. */
function replyTarget(target: EventTarget | null) {
  const focused = target instanceof HTMLElement ? target.closest('.msg') : null;
  const id = focused?.id.startsWith('msg-') ? focused.id.slice(4) : null;
  const found = id ? app.items.find((m) => m.id === id) : null;
  if (found && !found.retracted) return found;
  for (let i = app.items.length - 1; i >= 0; i--) if (!app.items[i].retracted) return app.items[i];
  return null;
}

/** Page keys scroll the timeline. A text field with its own scroll keeps them. */
function pageFree(e: KeyboardEvent): boolean {
  if (!chatFree()) return false;
  if (!isEditable(e.target)) return !isControl(e.target) || !!timeline()?.contains(e.target as Node);
  const box = e.target as HTMLElement;
  return inComposer(box) && box.scrollHeight <= box.clientHeight;
}

function run(action: Action, e: KeyboardEvent): boolean {
  switch (action) {
    case 'switcher':
      ui.switcherOpen = !ui.switcherOpen;
      return true;
    case 'settings':
      if (ui.settingsOpen) ui.settingsOpen = false;
      else ui.openSettings();
      return true;
    case 'shortcuts':
      ui.shortcutsOpen = !ui.shortcutsOpen;
      return true;
    case 'channelPrev':
    case 'channelNext':
      app.step(action === 'channelPrev' ? -1 : 1);
      return true;
    case 'unreadPrev':
    case 'unreadNext':
      app.stepUnread(action === 'unreadPrev' ? -1 : 1);
      return true;
    case 'spacePrev':
    case 'spaceNext':
      if (ui.overlays > 0) return false;
      stepSpace(action === 'spacePrev' ? -1 : 1);
      return true;
    case 'markAllRead':
      app.markAllRead();
      return true;
    case 'markRead':
      // Esc does its job only when nothing else wants the key.
      if (e.defaultPrevented || ui.overlays !== 0 || app.editingId || !escMarksRead(e.target)) {
        return false;
      }
      app.markRead();
      return false;
    case 'emoji':
    case 'gif':
    case 'upload':
      if (!(action === 'upload' ? chatFree() : popoverFree())) return true;
      ui.request(action);
      return true;
    case 'search':
    case 'pins':
      if (popoverFree()) ui.request(action);
      return true;
    case 'members':
      if (chatFree() && app.sideRail) ui.toggleMembers();
      return true;
    case 'reply': {
      if (!chatFree() || app.editingId) return false;
      if (isEditable(e.target) && !inComposer(e.target)) return false;
      const m = replyTarget(e.target);
      if (!m) return true;
      app.startReply(m);
      return true;
    }
    case 'pageUp':
    case 'pageDown': {
      if (!pageFree(e)) return false;
      const list = timeline();
      if (!list) return false;
      list.scrollBy({ top: (action === 'pageUp' ? -1 : 1) * list.clientHeight * 0.9 });
      return true;
    }
    case 'present': {
      if (!pageFree(e)) return false;
      const list = timeline();
      list?.scrollTo({ top: list.scrollHeight });
      return true;
    }
    case 'oldestUnread': {
      if (!pageFree(e)) return false;
      const line = timeline()?.querySelector('.divider.new');
      if (line) line.scrollIntoView({ block: 'start' });
      else if (app.dividerId) void jumpTo(app.dividerId);
      return true;
    }
  }
}

/** A letter typed with no field in focus goes to the composer. */
function focusOnType(e: KeyboardEvent): boolean {
  if (e.key.length !== 1 || e.key === ' ' || e.metaKey || e.ctrlKey || e.altKey) return false;
  if (!chatFree() || isEditable(e.target) || isControl(e.target)) return false;
  // The focus moves now, so the key lands in the box.
  return ui.request('composer');
}

/** Run the shortcut of a key press. Returns true when the press was a shortcut. */
export function handleKey(e: KeyboardEvent): boolean {
  if (e.isComposing) return false;
  const hit = findBinding(e, mac);
  if (!hit) return focusOnType(e);
  const repeatOk = hit.action === 'pageUp' || hit.action === 'pageDown' || hit.action.startsWith('channel');
  if (e.repeat && !repeatOk) {
    // A held key must not toggle a panel again and again.
    if (hit.action !== 'markRead') e.preventDefault();
    return true;
  }
  const done = run(hit.action, e);
  if (done) e.preventDefault();
  return done;
}
