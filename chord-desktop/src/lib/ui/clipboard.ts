// Copy text to the clipboard. `navigator.clipboard` works inside the webview when a click
// or a key press starts the call, which is the case for every menu row. If the webview
// refuses, an old-style copy from a hidden text field is the fallback.
import { ui } from './ui.svelte';

function fallback(text: string): boolean {
  const field = document.createElement('textarea');
  field.value = text;
  field.setAttribute('readonly', '');
  field.style.cssText = 'position:fixed;left:-9999px;top:0;opacity:0';
  document.body.appendChild(field);
  const held = document.activeElement as HTMLElement | null;
  field.select();
  let ok = false;
  try {
    ok = document.execCommand('copy');
  } catch {
    ok = false;
  }
  field.remove();
  held?.focus?.();
  return ok;
}

/** Copy `text`, then say `done` (or that the copy failed). */
export async function copyText(text: string, done = 'Copied.'): Promise<boolean> {
  let ok = false;
  try {
    await navigator.clipboard.writeText(text);
    ok = true;
  } catch {
    ok = fallback(text);
  }
  ui.say(ok ? done : 'Could not copy.');
  return ok;
}
