//! Builds the vendored libomemo-c (with its protobuf-c and curve25519 sources) into a
//! static library. No CMake and no system library. See `vendor/libomemo-c/VENDOR.md`.

use std::path::PathBuf;

/// The files of `src/CMakeLists.txt` of libomemo-c at the pinned tag.
const LIB_FILES: &[&str] = &[
    "LocalStorageProtocol.pb-c.c",
    "WhisperTextProtocol.pb-c.c",
    "OMEMO.pb-c.c",
    "vpool.c",
    "signal_protocol.c",
    "curve.c",
    "hkdf.c",
    "ratchet.c",
    "protocol.c",
    "session_state.c",
    "session_record.c",
    "session_pre_key.c",
    "session_builder.c",
    "session_cipher.c",
    "key_helper.c",
    "sender_key.c",
    "sender_key_state.c",
    "sender_key_record.c",
    "curve25519/curve25519-donna.c",
];

/// The files of `src/curve25519/CMakeLists.txt`: Ed25519 (ref10) and the XEdDSA additions.
/// Paths are relative to `src/curve25519/ed25519`, without the `.c` suffix.
const ED_FILES: &[&str] = &[
    "fe_0",
    "fe_1",
    "fe_add",
    "fe_cmov",
    "fe_copy",
    "fe_frombytes",
    "fe_invert",
    "fe_isnegative",
    "fe_isnonzero",
    "fe_mul",
    "fe_neg",
    "fe_pow22523",
    "fe_sq",
    "fe_sq2",
    "fe_sub",
    "fe_tobytes",
    "ge_add",
    "ge_double_scalarmult",
    "ge_frombytes",
    "ge_madd",
    "ge_msub",
    "ge_p1p1_to_p2",
    "ge_p1p1_to_p3",
    "ge_p2_0",
    "ge_p2_dbl",
    "ge_p3_0",
    "ge_p3_dbl",
    "ge_p3_to_cached",
    "ge_p3_to_p2",
    "ge_p3_tobytes",
    "ge_precomp_0",
    "ge_scalarmult_base",
    "ge_sub",
    "ge_tobytes",
    "open",
    "sc_muladd",
    "sc_reduce",
    "sign",
    "additions/compare",
    "additions/curve_sigs",
    "additions/ed_sigs",
    "additions/elligator",
    "additions/fe_edy_to_montx",
    "additions/fe_isequal",
    "additions/fe_isreduced",
    "additions/fe_mont_rhs",
    "additions/fe_montx_to_edy",
    "additions/fe_sqrt",
    "additions/ge_isneutral",
    "additions/ge_montx_to_p3",
    "additions/ge_neg",
    "additions/ge_p3_to_montx",
    "additions/ge_scalarmult",
    "additions/ge_scalarmult_cofactor",
    "additions/keygen",
    "additions/open_modified",
    "additions/sc_clamp",
    "additions/sc_cmov",
    "additions/sc_neg",
    "additions/sign_modified",
    "additions/utility",
    "additions/generalized/ge_p3_add",
    "additions/generalized/gen_eddsa",
    "additions/generalized/gen_labelset",
    "additions/generalized/gen_veddsa",
    "additions/generalized/gen_x",
    "additions/generalized/point_isreduced",
    "additions/generalized/sc_isreduced",
    "additions/xeddsa",
    "additions/zeroize",
    "nacl_sha512/blocks",
    "nacl_sha512/hash",
    "tests/internal_fast_tests",
];

fn main() {
    let root =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../vendor/libomemo-c");
    let src = root.join("src");
    let ed = src.join("curve25519/ed25519");

    let mut files: Vec<PathBuf> = LIB_FILES.iter().map(|n| src.join(n)).collect();
    files.extend(ED_FILES.iter().map(|n| ed.join(format!("{n}.c"))));
    files.push(root.join("protobuf-c/protobuf-c/protobuf-c.c"));

    let mut build = cc::Build::new();
    build
        .files(&files)
        .include(&src)
        .include(root.join("protobuf-c"))
        .include(src.join("curve25519"))
        .include(&ed)
        .include(ed.join("nacl_includes"))
        .include(ed.join("additions"))
        .include(ed.join("additions/generalized"))
        .include(ed.join("nacl_sha512"))
        .include(ed.join("tests"))
        // The vendored C code is not ours. Do not print its warnings.
        .warnings(false)
        .extra_warnings(false)
        .flag_if_supported("-fvisibility=hidden");
    if std::env::var("CARGO_CFG_TARGET_ENDIAN").as_deref() == Ok("big") {
        build.define("WORDS_BIGENDIAN", None);
    }
    build.compile("omemo_c");

    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    println!("cargo:rerun-if-changed=build.rs");
}
