# Chord themes

A theme is CSS. Paste it in Settings > Appearance > Import theme.

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
