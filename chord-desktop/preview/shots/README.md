# Desktop screenshots

`capture.mjs` takes the marketing screenshots of the desktop preview and writes them to
`docs/brand/screenshots/desktop/`. The data is the showcase story in
`docs/brand/showcase/README.md`.

1. Build the preview: `npm run build:preview` in `chord-desktop`.
2. Serve it on port 4801: `python3 -m http.server 4801 --bind 127.0.0.1 -d build-preview`.
3. In this folder: `npm install`, then `npx playwright install chromium` (or set
   `CHROMIUM=/usr/bin/chromium-browser` to use the system Chromium).
4. Run `npm run shots`. To take only some shots, give their names: `node capture.mjs main home`.

The shots use compact mode and a 14 px font. Set `PORT` if the server uses another port.
