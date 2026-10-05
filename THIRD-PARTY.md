# Third-party code in the source tree

The Chord source files are MIT (see `LICENSE`). The folder `vendor/libomemo-c` holds code from
other projects. The OMEMO part of the app links it. The built app is therefore distributed
under the GPL-3.0 (see the License section of `README.md`).

| Part | Where | License |
|---|---|---|
| libomemo-c 0.5.1 | `vendor/libomemo-c/src` | GPL-3.0, with an additional permission for the Apple App Store. Full text: `vendor/libomemo-c/LICENSE`. The permission text is in `vendor/libomemo-c/README.md`. |
| protobuf-c 1.5.1 | `vendor/libomemo-c/protobuf-c` | BSD-2-Clause. Text: `vendor/libomemo-c/protobuf-c/LICENSE`. |
| curve25519-donna | `vendor/libomemo-c/src/curve25519/curve25519-donna.c` | BSD-3-Clause, Google Inc. 2008. The text is in the header of the file. |
| Ed25519 ref10 and SHA-512 | `vendor/libomemo-c/src/curve25519/ed25519` | Public domain (D. J. Bernstein and others). |
| uthash, utarray, utlist | `vendor/libomemo-c/src` | BSD-style, Troy D. Hanson. The text is in the header of each file. |
| vpool | `vendor/libomemo-c/src/vpool.[ch]` | ISC-style, Alexey Vatchenko. The text is in the header of each file. |

`vendor/libomemo-c/VENDOR.md` says which version we copied and how we build it.

The Android app shows the GPL-3.0 text and the BSD notices in its About page. The files are in
`chord-android/app/src/main/assets/licenses`. The other bundled parts (fonts, icons, emoji data)
have their licenses listed in the About pages of the apps.
