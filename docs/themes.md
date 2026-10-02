# Chord themes

A theme is CSS. Add it in Settings > Appearance > Import theme. You can give a link or paste the CSS.

```css
/**
 * @name My theme
 * @author Me
 * @mode dark
 * @default blue
 * @accent blue Blue
 * @accent pink Pink
 */
:root {
  --surface-100: #1e1e2e; /* message pane */
  --surface-200: #181825; /* cards, channel list */
  --surface-300: #313244; /* inputs, tiles, hovered rows */
  --surface-rail: #11111b; /* the space rail and the user panel */
  --surface-side: #181825; /* the channel list and the member list */
  --line: #45475a;
  --line-strong: #585b70; /* the edge of a column, the chat header, and the composer */
  --ink: #cdd6f4;
  --ink-muted: #a6adc8;
  --brand: #89b4fa;       /* buttons, mentions, the selected accent */
  --brand-soft: color-mix(in srgb, var(--brand) 20%, #1e1e2e);
  --brand-ink: var(--brand);
  --on-brand: #11111b;
  --accent: #89b4fa;      /* links, focus rings */
  --online: #a6e3a1;
  --away: #f9e2af;        /* the away status, not the accent */
  --danger: #f38ba8;
  --on-danger: #11111b;
}
:root[data-accent="blue"] { --brand: #89b4fa; }
:root[data-accent="pink"] { --brand: #f5c2e7; }
```

- `@mode` is `dark` or `light`. Without it, Chord reads `--surface-100`.
- Each `[data-accent="id"]` rule is an accent. `@accent id Name` names it and sets the order.
- A token that the theme does not set keeps its Chord Dark value.
- `--surface-rail`, `--surface-side` and `--line-strong` are optional. Without them, the rail uses `--surface-300`, the lists use `--surface-200`, and the strong line is a mix of `--ink` and `--surface-100`. Give the rail, the lists and the chat three clearly different surfaces. Then the areas stay easy to tell apart.
- The rest of the CSS can style any part of the app.
- An imported theme cannot load a file from outside the app. Chord removes `@import`, `image-set()`, `image()`, `src()`, `cross-fade()`, `expression()`, `attr()` that builds a `url`, and every `url()` that is not a `data:` URL or a `#fragment`. A `url()` that Chord removes becomes an empty one. Without this, a theme could send requests to its author and learn which links and file names are on your screen. The import dialog and the update question tell you how many requests Chord blocked. The built-in themes are not checked.
- A theme cannot hide, move or cover the buttons of a question such as "Open this link?".

The built-in themes are in `chord-desktop/src/lib/theme/themes/`.

## Install from a link

The Import dialog opens on the "From a link" tab. Give the https link of a CSS file and select Add.

- A GitHub page link (`github.com/user/repo/blob/main/theme.css`) turns into the raw link of the file.
- Links to `raw.githubusercontent.com`, gist raw files, `cdn.jsdelivr.net` and other https hosts work as they are.
- Chord downloads the file in the app, not in the webview. The file must be text and 256 KB at most.
- A linked theme shows a link icon and the host name on its card.
- At each launch Chord shows the stored CSS first. Then it fetches each linked theme again. If the new CSS is valid and different, Chord does not use it yet. The theme card shows how many lines changed, with Update and Skip. Update applies the new CSS. Skip keeps the old CSS until the link changes again. The author of a file can change it at any time, so the update always waits for you.
- If a fetch fails, Chord keeps the old CSS. It writes a line in the log and shows no error.
- To stop the updates, remove the theme. Then paste the CSS if you want a fixed copy.

In the browser preview, the fetch runs in the browser. It works only if the site allows it (CORS). The raw GitHub host does.
