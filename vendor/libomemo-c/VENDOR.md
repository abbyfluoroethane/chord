# libomemo-c (vendored)

This folder holds a copy of the source of libomemo-c. `chord-omemo/build.rs` builds it with
the `cc` crate. Do not edit the files. To update, copy a new tag over them. Then change this
file.

| | |
|---|---|
| Project | libomemo-c, a fork of libsignal-protocol-c that adds OMEMO 2 |
| URL | https://github.com/dino/libomemo-c |
| Tag | v0.5.1 (an annotated tag, tag object `861c8cb391f457cd0f6ab69272e25a38db8b73b2`) |
| Commit | `1f0b37fee3ebefc46c32f882623481899a1ba568` (2025-02-28) |
| Copied on | 2026-10-05 |
| License | GPL-3.0, with an additional permission for the Apple App Store. See `LICENSE` and the "License" block of `README.md`. |

## What is here

- `src/`: all C sources and headers of the library, with the `curve25519` folder
  (curve25519-donna, Ed25519 ref10, XEdDSA additions). We removed the build files
  (`CMakeLists.txt`, `meson.build`, `*.def`, `*.pc.in`) and `internal_slow_tests.*`. We
  added the three `.proto` files from `protobuf/` into `src/` for reference.
- `protobuf-c/`: `protobuf-c.c` and `protobuf-c.h` of protobuf-c 1.5.1 (the release that
  `subprojects/protobuf-c.wrap` of libomemo-c names, SHA-256 of the tarball
  `20d1dc257da96f8ddff8be4dd9779215bbd0a6069ed53bbe9de38fa7629be06b`), with its `LICENSE`
  (BSD-2-Clause). The generated `*.pb-c.[ch]` files are in `src/`.
- `LICENSE`, `README.md`: from libomemo-c.

## Licenses inside

| Part | License |
|---|---|
| libomemo-c (src/) | GPL-3.0, plus the Apple App Store permission (see `README.md`) |
| protobuf-c | BSD-2-Clause (`protobuf-c/LICENSE`) |
| curve25519-donna (`src/curve25519/curve25519-donna.c`) | BSD-3-Clause, Google Inc. 2008 (header of the file) |
| Ed25519 ref10, SHA-512 (`src/curve25519/ed25519`) | Public domain (D. J. Bernstein et al.). `nacl_sha512/hash.c` says "Public domain." |
| `src/utarray.h`, `src/uthash.h`, `src/utlist.h` | BSD-style, Troy D. Hanson |
| `src/vpool.[ch]` | ISC-style, Alexey Vatchenko |

## How it is built

`chord-omemo/build.rs` lists the files of `src/CMakeLists.txt` and
`src/curve25519/CMakeLists.txt`. It makes one static library, `omemo_c`. It does not use
OpenSSL, CommonCrypto or any system library. The crypto provider (random, HMAC-SHA-256,
SHA-512, AES) comes from RustCrypto crates, in `chord-omemo/src/provider.rs`.

Tested: macOS (aarch64, Apple clang). Not tested yet:

- Linux: the `cc` crate uses `cc` or `gcc`. No extra flag or library should be needed.
- Windows MSVC: not tested. The sources have `_MSC_VER` branches. Expect to add
  `HAVE_SECUREZEROMEMORY` and `windows.h` for `signal_explicit_bzero` in
  `signal_protocol.c`, and to check `ssize_t` and `strdup` uses.
- Android NDK: the `cc` crate takes the NDK clang from the environment (for example with
  `cargo ndk`). No extra flag should be needed. Test the four ABIs.
- wasm32: not built. `chord-omemo` is for the native feature of `chord-core` only.

The C code keeps its `assert` calls on. The build does not set `NDEBUG`. The asserts catch
wrong use of the C API. A failed assert stops the process. That is better than undefined
behavior.

## Known limits

- libomemo-c frees some secret buffers with `signal_buffer_free`, not
  `signal_buffer_bzero_free`. Key bytes can stay in freed heap memory. Only a patch can
  change that. Our own buffers get wiped.
- The library is not thread-safe by itself. Each call of `chord-omemo` makes its own
  context. Calls from many threads are safe.
