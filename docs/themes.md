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
  --surface-300: #313244; /* rail, inputs, hovered rows */
  --line: #45475a;
  --ink: #cdd6f4;
  --ink-muted: #a6adc8;
  --brand: #89b4fa;       /* buttons, mentions, the selected accent */
  --brand-soft: color-mix(in srgb, var(--brand) 20%, #1e1e2e);
  --brand-ink: var(--brand);
  --on-brand: #11111b;
  --accent: #89b4fa;      /* links, focus rings */
  --online: #a6e3a1;
  --danger: #f38ba8;
  --on-danger: #11111b;
}
:root[data-accent="blue"] { --brand: #89b4fa; }
:root[data-accent="pink"] { --brand: #f5c2e7; }
```

- `@mode` is `dark` or `light`. Without it, Chord reads `--surface-100`.
- Each `[data-accent="id"]` rule is an accent. `@accent id Name` names it and sets the order.
- A token that the theme does not set keeps its Chord Dark value.
- The rest of the CSS can style any part of the app.

The built-in themes are in `chord-desktop/src/lib/theme/themes/`.

## Install from a link

The Import dialog opens on the "From a link" tab. Give the https link of a CSS file and select Add.

- A GitHub page link (`github.com/user/repo/blob/main/theme.css`) turns into the raw link of the file.
- Links to `raw.githubusercontent.com`, gist raw files, `cdn.jsdelivr.net` and other https hosts work as they are.
- Chord downloads the file in the app, not in the webview. The file must be text and 256 KB at most.
- A linked theme shows a link icon and the host name on its card.
- At each launch Chord shows the stored CSS first. Then it fetches each linked theme again. If the new CSS is valid, it replaces the old CSS. An active theme changes at once.
- If a fetch fails, Chord keeps the old CSS. It writes a line in the log and shows no error.
- To stop the updates, remove the theme. Then paste the CSS if you want a fixed copy.

In the browser preview, the fetch runs in the browser. It works only if the site allows it (CORS). The raw GitHub host does.
