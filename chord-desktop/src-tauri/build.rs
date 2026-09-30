use std::path::Path;

/// The KLIPY key for GIF search (src/gif.rs), set at build time. The key is never in
/// git. A build takes it from the CHORD_KLIPY_KEY variable, else from the git-ignored
/// file dev/klipy/.env at the repository root. Release builds get it from a CI secret.
/// See docs/gifs.md.
fn klipy_key() {
    println!("cargo:rerun-if-env-changed=CHORD_KLIPY_KEY");
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/klipy/.env");
    println!("cargo:rerun-if-changed={}", file.display());
    if std::env::var("CHORD_KLIPY_KEY").is_ok_and(|k| !k.trim().is_empty()) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(&file) else {
        return;
    };
    let key = text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("CHORD_KLIPY_KEY=")?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_owned())
    });
    if let Some(key) = key {
        println!("cargo:rustc-env=CHORD_KLIPY_KEY={key}");
    }
}

fn main() {
    klipy_key();
    tauri_build::build()
}
