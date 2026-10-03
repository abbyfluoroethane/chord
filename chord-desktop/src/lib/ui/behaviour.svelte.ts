// The page side of the app behaviour: auto-away, the unread dot of the tray icon, and the
// status items of the tray menu. Call `useBehaviour` once, while a component starts.
import { untrack } from 'svelte';
import { app } from './app.svelte';
import { watchAutoAway } from './autoaway';
import { api, live } from './bridge';
import { browserEnv } from './idle';
import { prefs } from './prefs.svelte';

export function useBehaviour() {
  // Away when nobody uses the computer. The effect starts again when the minutes change.
  $effect(() => {
    if (!prefs.autoAway) return;
    const minutes = prefs.autoAwayMinutes;
    return untrack(() =>
      watchAutoAway(
        { show: () => app.me.show, setShow: (s) => app.setShow(s) },
        minutes,
        live
          ? {
              ...browserEnv,
              systemIdle: async () => (await api()).systemIdleSeconds()
            }
          : undefined
      )
    );
  });

  // The dot and the tooltip of the tray icon follow the unread total.
  $effect(() => {
    const n = app.totalUnread;
    if (!live) return;
    void api()
      .then((b) => b.setTrayUnread(n))
      .catch(() => {
        /* A tray that fails does no harm. */
      });
  });

  // A status that the user picks in the tray menu.
  $effect(() => {
    if (!live) return;
    let stop: (() => void) | undefined;
    let gone = false;
    void api()
      .then((b) => b.onTrayStatus((status) => app.setShow(status)))
      .then((off) => {
        if (gone) off();
        else stop = off;
      })
      .catch(() => {
        /* No events: the tray menu does nothing. */
      });
    return () => {
      gone = true;
      stop?.();
    };
  });
}
