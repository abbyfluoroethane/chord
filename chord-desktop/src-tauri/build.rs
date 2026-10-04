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

/// The output of a git command in the repository, or None.
fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// CHORD_VERSION and CHORD_COMMIT for the About page. A release sets CHORD_VERSION from the
/// tag desktop-v<version>. Without it, a build takes the last desktop-v tag and adds "+dev".
/// See docs/releasing.md.
fn version() {
    println!("cargo:rerun-if-env-changed=CHORD_VERSION");
    // A new commit, checkout or tag changes the answer.
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
    if let Some(dir) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        println!("cargo:rerun-if-changed={dir}/packed-refs");
        println!("cargo:rerun-if-changed={dir}/refs/tags");
    }
    let version = std::env::var("CHORD_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            let tag = git(&["describe", "--tags", "--abbrev=0", "--match", "desktop-v*"])?;
            Some(format!("{}+dev", tag.strip_prefix("desktop-v")?))
        })
        .unwrap_or_else(|| "0.0.0+dev".to_owned());
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=CHORD_VERSION={version}");
    println!("cargo:rustc-env=CHORD_COMMIT={commit}");
}

fn main() {
    klipy_key();
    version();
    tauri_build::build()
}
