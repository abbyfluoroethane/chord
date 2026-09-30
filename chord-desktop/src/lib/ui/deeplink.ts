// The xmpp: links that the OS gives to the app. The Tauri deep-link plugin reports two cases:
// the link that started the app (`getCurrent`), and each link while the app runs
// (`onOpenUrl`). Both go to `xmppLinks.open`, which waits for the sign-in and asks the user.
// The browser preview has no OS links, so it does nothing there.
import { live } from './bridge';
import { xmppLinks } from './xmpplinks.svelte';

/** Start to listen. Returns a function that stops it. */
export async function startDeepLinks(): Promise<() => void> {
  if (!live) return () => undefined;
  try {
    const { getCurrent, onOpenUrl } = await import('@tauri-apps/plugin-deep-link');
    const handle = (urls: string[] | null | undefined) => {
      for (const url of urls ?? []) xmppLinks.open(url, true);
    };
    const stop = await onOpenUrl(handle);
    handle(await getCurrent());
    return stop;
  } catch (e) {
    console.warn('xmpp: links are not available', e);
    return () => undefined;
  }
}
