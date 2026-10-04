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
    // A build without .git (the Flatpak build) gets the commit and the commit time (seconds
    // since 1970) from CHORD_COMMIT and CHORD_COMMIT_TIME. See packaging/flatpak/space.foid.chord.yml.
    println!("cargo:rerun-if-env-changed=CHORD_COMMIT");
    println!("cargo:rerun-if-env-changed=CHORD_COMMIT_TIME");
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let commit = env("CHORD_COMMIT")
        .or_else(|| git(&["rev-parse", "--short", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());
    let channel = channel_of(&version);
    let time = env("CHORD_COMMIT_TIME")
        .or_else(|| git(&["show", "-s", "--format=%ct", "HEAD"]))
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    println!("cargo:rustc-env=CHORD_VERSION={version}");
    println!("cargo:rustc-env=CHORD_COMMIT={commit}");
    println!("cargo:rustc-env=CHORD_CHANNEL={channel}");
    println!(
        "cargo:rustc-env=CHORD_BUILD={}",
        build_number(time, channel)
    );
}

/// The update channel of a version: stable, beta, nightly, or dev for a build that is not a release.
fn channel_of(version: &str) -> &'static str {
    let (core, meta) = version.split_once('+').unwrap_or((version, ""));
    if meta.contains("dev") {
        "dev"
    } else if core.contains("-beta.") {
        "beta"
    } else if core.contains("-nightly.") {
        "nightly"
    } else {
        "stable"
    }
}

/// The build number: the minutes from 2026-01-01 to the commit, times 4, plus 2 for a release,
/// 1 for a beta, 0 for a nightly or a dev build. A newer commit always has a higher number, in
/// any channel, so the updater can move between channels. The same rule is in
/// chord-android/app/build.gradle.kts.
fn build_number(commit_time: i64, channel: &str) -> i64 {
    let minutes = ((commit_time - 1_767_225_600) / 60).max(0);
    let slot = match channel {
        "stable" => 2,
        "beta" => 1,
        _ => 0,
    };
    minutes * 4 + slot
}

fn main() {
    klipy_key();
    version();
    tauri_build::build()
}
