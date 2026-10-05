# Third-party code in the source tree

The Chord source files are MIT (see `LICENSE`). One part of the source tree comes from
another project.

| Part | Where | License |
|---|---|---|
| vodozemac 0.11.1 (fork) | `chord-omemo/src/ratchet` | Apache-2.0. The text is in `chord-omemo/src/ratchet/LICENSE`. vodozemac has no NOTICE file. |

The fork is the Double Ratchet code of vodozemac, changed for OMEMO 2. Each changed file says so
in its header (Apache-2.0, section 4). `chord-omemo/src/ratchet/FORK.md` has the upstream
tag and commit, the list of copied files, and every change.

The Rust crates that Chord uses have their own licenses (MIT, Apache-2.0, or BSD). `Cargo.lock`
lists them.

The Android app shows the Apache-2.0 text in its About page. The file is in
`chord-android/app/src/main/assets/licenses`. The other bundled parts (fonts, icons, emoji data)
have their licenses listed in the About pages of the apps.
