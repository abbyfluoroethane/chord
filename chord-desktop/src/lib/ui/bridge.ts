// The data source switch. Inside Tauri the UI talks to the bridge (`$lib/chord`).
// In a browser (npm run dev, the static preview) it keeps the sample data.
//
// The bridge loads on first use, so the preview never runs Tauri code at load time.

/** True when the page runs inside the Tauri webview. */
export const live: boolean = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

type Bridge = typeof import('$lib/chord');
let loaded: Promise<Bridge> | null = null;

/** The typed bridge. Only call it when `live` is true. */
export function api(): Promise<Bridge> {
  loaded ??= import('$lib/chord');
  return loaded;
}
