# GIF search

Chord searches GIFs with the KLIPY API. A build needs a KLIPY API key. The key is not in git.

## Build from source with GIFs

1. Get a free API key from KLIPY (klipy.com, for developers).
2. Put it in `dev/klipy/.env` at the repository root:
   ```
   CHORD_KLIPY_KEY=your-key
   ```
   Git ignores this file. Give it owner-only access: `chmod 600 dev/klipy/.env`.
3. Build as normal. The build reads the file and puts the key in the app.

A `CHORD_KLIPY_KEY` variable in the environment of the build wins over the file. At run time, the same variable wins over the key in the app.

Without a key, the app works, and the GIF tab says that GIF search is off.

## Release builds

The release workflow gives the key from a CI secret named `CHORD_KLIPY_KEY`. Anyone can read a key out of an app file, so do not use a key that you cannot replace.

## Keep the key out of the app (a proxy)

A key in the app file can be read and used by anyone, and they can use up its quota. The KLIPY documentation pages were not readable when this was written (the site refused the request), so Chord does not assume that KLIPY limits a key by domain or by app. To ship no key, run a small server that holds the key and limits the rate of each client. Give its https address to the build in `CHORD_GIF_PROXY`. At run time, the same variable wins over the address in the app.

The app then sends `GET {proxy}/gifs/search` and `GET {proxy}/gifs/trending` with the same query as KLIPY (`page`, `per_page`, `format_filter`, `q`), and no key. The server adds the key in the path of the KLIPY URL and returns the answer as it is. `CHORD_GIF_PROXY` wins over `CHORD_KLIPY_KEY`. The server itself is not part of this repository.
