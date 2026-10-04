# Desktop screenshots

`capture.mjs` takes the marketing screenshots of the desktop preview and writes them to
`docs/brand/screenshots/desktop/`. The data is the showcase story in
`docs/brand/showcase/README.md`.

1. Build the preview: `npm run build:preview` in `chord-desktop`.
2. Serve it on port 4801: `python3 -m http.server 4801 --bind 127.0.0.1 -d build-preview`.
3. In this folder: `npm install`, then `npx playwright install chromium` (or set
   `CHROMIUM=/usr/bin/chromium-browser` to use the system Chromium).
4. Run `npm run shots`. To take only some shots, give their names: `node capture.mjs music join`.

The shots use cozy display and the default font size, zoomed in: a 1000x625 window at a scale
of 2.88, so a full-window shot is 2880x1800 and the app looks like it does at about 150 %.
Shots 01 to 04 are the whole window. Shots 05 to 10 are crops around one part. Every shot
comes in dark and light. Set `PORT` if the server uses another port.
