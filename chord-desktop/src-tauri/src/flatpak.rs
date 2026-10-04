//! Chord in a Flatpak. See docs/updates.md.
//!
//! On Linux Chord ships only as a Flatpak. The Flatpak branch is the update channel:
//! `space.foid.chord//stable`, `//beta` or `//nightly`. The app does not switch the branch
//! itself: the page shows the one-time command for it.
//!
//! The updates come from the Flatpak portal (`org.freedesktop.portal.Flatpak`, version 2 or
//! newer). An update monitor tells the app when the remote of its branch has a new commit.
//! `Update` installs it with progress, and `Spawn` with the latest-version flag starts the new
//! version. The portal gives no version text, only commits.

// Only Linux calls the portal. The parts without it stay testable on each platform.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use serde::Serialize;

use crate::updates::UpdateInfo;

/// The Flatpak app ID. It is the identifier in tauri.conf.json.
pub const APP_ID: &str = "space.foid.chord";

/// The self-hosted Flatpak repository of the nightly builds.
pub const NIGHTLY_REMOTE: &str = "chord-nightly";
pub const NIGHTLY_REPO: &str =
    "https://bigaouette.com/chord-nightly/flatpak/chord-nightly.flatpakrepo";

/// Flathub has the stable branch. Flathub Beta has the beta branch.
pub const FLATHUB_REMOTE: &str = "flathub";
pub const FLATHUB_BETA_REMOTE: &str = "flathub-beta";
pub const FLATHUB_BETA_REPO: &str = "https://flathub.org/beta-repo/flathub-beta.flatpakrepo";

/// The file that Flatpak puts at the root of each sandbox.
const INFO_PATH: &str = "/.flatpak-info";

/// The Flatpak install that runs this app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatpakInfo {
    /// `[Application] name`.
    pub app_id: String,
    /// `[Instance] branch`: the channel, for example `beta`. Empty when Flatpak did not say.
    pub branch: String,
    /// `[Instance] arch`.
    pub arch: Option<String>,
    /// `[Instance] app-commit`: the OSTree commit of the running app.
    pub commit: Option<String>,
    /// `[Instance] flatpak-version`.
    pub flatpak_version: Option<String>,
    /// `[Instance] build`: a `flatpak build` run (flatpak-builder --run). It has no updates.
    #[serde(skip)]
    pub build: bool,
    /// The commands that install each channel, for the page.
    pub switch: Vec<SwitchCommand>,
}

/// The one-time command that installs the branch of a channel, and the command that makes
/// that branch the one that runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchCommand {
    pub channel: &'static str,
    pub install: String,
    pub make_current: String,
}

/// The value of `key` in `[group]` of a GLib key file. Comments and other groups are skipped.
fn key_value<'a>(text: &'a str, group: &str, key: &str) -> Option<&'a str> {
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_group = name == group;
            continue;
        }
        if !in_group {
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim());
        }
    }
    None
}

/// Read the text of `/.flatpak-info`. A runtime-only sandbox names its runtime in
/// `[Runtime]` instead of `[Application]`: that is not this app.
pub fn parse_info(text: &str) -> Option<FlatpakInfo> {
    let app_id = key_value(text, "Application", "name")?;
    if app_id.is_empty() {
        return None;
    }
    let instance = |key: &str| {
        key_value(text, "Instance", key)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    Some(FlatpakInfo {
        app_id: app_id.to_owned(),
        branch: instance("branch").unwrap_or_default(),
        arch: instance("arch"),
        commit: instance("app-commit"),
        flatpak_version: instance("flatpak-version"),
        build: instance("build").is_some_and(|v| v == "true"),
        switch: switch_commands(),
    })
}

/// The Flatpak install that runs this app, or None outside Flatpak. Read once.
pub fn instance() -> Option<&'static FlatpakInfo> {
    static INFO: std::sync::OnceLock<Option<FlatpakInfo>> = std::sync::OnceLock::new();
    INFO.get_or_init(|| {
        match std::fs::read_to_string(INFO_PATH) {
            Ok(text) => parse_info(&text),
            // FLATPAK_ID without the file: Flatpak, but the branch is unknown.
            Err(_) => std::env::var("FLATPAK_ID")
                .ok()
                .filter(|id| !id.is_empty())
                .map(|app_id| FlatpakInfo {
                    app_id,
                    branch: String::new(),
                    arch: None,
                    commit: None,
                    flatpak_version: None,
                    build: false,
                    switch: switch_commands(),
                }),
        }
    })
    .as_ref()
}

/// Can this Flatpak update itself? Not a `flatpak build` run.
pub fn can_update(info: &FlatpakInfo) -> bool {
    !info.build
}

/// The command that adds the remote of a channel, when needed, and installs its branch.
pub fn install_command(channel: &str) -> Option<String> {
    let (remote, repo) = match channel {
        "stable" => (FLATHUB_REMOTE, None),
        "beta" => (FLATHUB_BETA_REMOTE, Some(FLATHUB_BETA_REPO)),
        "nightly" => (NIGHTLY_REMOTE, Some(NIGHTLY_REPO)),
        _ => return None,
    };
    let install = format!("flatpak install {remote} {APP_ID}//{channel}");
    Some(match repo {
        Some(repo) => format!("flatpak remote-add --if-not-exists {remote} {repo} && {install}"),
        None => install,
    })
}

/// The command that picks which installed branch runs.
pub fn make_current_command(branch: &str) -> String {
    format!("flatpak make-current {APP_ID} {branch}")
}

fn switch_commands() -> Vec<SwitchCommand> {
    ["stable", "beta", "nightly"]
        .into_iter()
        .filter_map(|channel| {
            Some(SwitchCommand {
                channel,
                install: install_command(channel)?,
                make_current: make_current_command(channel),
            })
        })
        .collect()
}

/// What the update monitor last said: the commit that runs, the commit that is installed,
/// and the commit of the remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub running: String,
    pub local: String,
    pub remote: String,
}

impl Found {
    /// Is there a newer commit, installed or not?
    pub fn is_update(&self) -> bool {
        self.local != self.running || self.remote != self.local
    }

    /// Is the newer commit already installed? Then only a restart is needed.
    pub fn installed(&self) -> bool {
        self.local != self.running && self.remote == self.local
    }

    /// The page view. The portal gives no version, so the version is empty.
    pub fn info(&self, branch: &str) -> UpdateInfo {
        let commit = if self.remote.is_empty() {
            &self.local
        } else {
            &self.remote
        };
        UpdateInfo {
            version: String::new(),
            build: 0,
            channel: (!branch.is_empty()).then(|| branch.to_owned()),
            commit: (!commit.is_empty()).then(|| commit.chars().take(10).collect()),
            notes: None,
            pub_date: None,
            release_url: None,
            installed: self.installed(),
        }
    }
}

/// The progress of an update as a part of the whole: each operation counts 100.
pub fn overall_progress(op: Option<u32>, n_ops: Option<u32>, progress: Option<u32>) -> (u64, u64) {
    let n_ops = u64::from(n_ops.unwrap_or(0).max(1));
    let op = u64::from(op.unwrap_or(0)).min(n_ops - 1);
    let progress = u64::from(progress.unwrap_or(0)).min(100);
    (op * 100 + progress, n_ops * 100)
}

/// The error code and the text for the page of a failed update. `name` is the D-Bus error
/// name that the portal sends, `message` its text.
pub fn failure(name: &str, message: &str) -> (&'static str, String) {
    let lower = message.to_lowercase();
    if lower.contains("new permissions") {
        return (
            "updatePermissions",
            "This update needs new permissions. Update Chord in your software app.".into(),
        );
    }
    if name.ends_with(".AccessDenied") {
        return (
            "updateDenied",
            "Chord may not update itself. Allow it in the privacy settings of the system, or \
             update Chord in your software app."
                .into(),
        );
    }
    if ["RemoteNotFound", "NotInstalled", "RefNotFound"]
        .iter()
        .any(|n| name.ends_with(n))
        || lower.contains("no remote")
        || lower.contains("not installed")
    {
        return (
            "updateNoRemote",
            "This copy of Chord was not installed from a Flatpak repository, so it cannot update \
             itself."
                .into(),
        );
    }
    if [
        "resolve",
        "network",
        "connect",
        "timed out",
        "timeout",
        "offline",
    ]
    .iter()
    .any(|w| lower.contains(w))
    {
        return (
            "updateNetwork",
            "Cannot reach the update server. Check the network connection and try again.".into(),
        );
    }
    let text = if message.is_empty() {
        "The update failed.".to_owned()
    } else {
        message.to_owned()
    };
    ("update", text)
}

#[cfg(target_os = "linux")]
pub use portal::{Updater, check, install, restart};

/// The portal calls. Only Linux has Flatpak.
#[cfg(target_os = "linux")]
mod portal {
    use std::collections::HashMap;
    use std::os::fd::OwnedFd;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use ashpd::flatpak::update_monitor::{UpdateMonitor, UpdateOptions, UpdateStatus};
    use ashpd::flatpak::{CreateUpdateMonitorOptions, Flatpak, SpawnFlags, SpawnOptions};
    use futures_util::StreamExt;
    use tauri::{AppHandle, Emitter, Manager};

    use super::{Found, failure, instance, overall_progress};
    use crate::error::{ChordError, Res};
    use crate::state::lock;
    use crate::updates::{UPDATE_EVENT, UpdateInfo};

    /// The update monitor needs version 2 of the portal.
    const MIN_VERSION: u32 = 2;

    /// The longest wait for the next progress message of an install.
    const PROGRESS_TIMEOUT: Duration = Duration::from_secs(10 * 60);

    struct Portal {
        flatpak: Flatpak,
        monitor: UpdateMonitor,
    }

    /// The portal connection and the last news of the update monitor.
    #[derive(Default)]
    pub struct Updater {
        portal: tokio::sync::Mutex<Option<Arc<Portal>>>,
        found: Mutex<Option<Found>>,
    }

    fn portal_error(e: ashpd::Error) -> ChordError {
        match e {
            ashpd::Error::PortalNotFound(_) | ashpd::Error::RequiresVersion(..) => ChordError::new(
                "updatePortal",
                "The Flatpak portal of this system cannot update apps. Update Chord in \
                     your software app.",
            ),
            e => ChordError::new("update", e.to_string()),
        }
    }

    fn branch() -> &'static str {
        instance().map_or("", |i| i.branch.as_str())
    }

    impl Updater {
        /// The portal and its update monitor. The first call connects and starts to listen.
        async fn portal(&self, app: &AppHandle) -> Res<Arc<Portal>> {
            let mut slot = self.portal.lock().await;
            if let Some(portal) = slot.as_ref() {
                return Ok(portal.clone());
            }
            let flatpak = Flatpak::new().await.map_err(portal_error)?;
            if flatpak.version() < MIN_VERSION {
                return Err(portal_error(ashpd::Error::RequiresVersion(
                    MIN_VERSION,
                    flatpak.version(),
                )));
            }
            let monitor = flatpak
                .create_update_monitor(CreateUpdateMonitorOptions::default())
                .await
                .map_err(portal_error)?;
            let portal = Arc::new(Portal { flatpak, monitor });
            listen(app.clone(), portal.clone());
            *slot = Some(portal.clone());
            Ok(portal)
        }

        fn set_found(&self, found: Option<Found>) {
            *lock(&self.found) = found;
        }

        fn found(&self) -> Option<Found> {
            lock(&self.found).clone()
        }
    }

    /// Keep the news of the update monitor, and tell the page about an update. The portal
    /// looks for updates about twice an hour.
    fn listen(app: AppHandle, portal: Arc<Portal>) {
        tauri::async_runtime::spawn(async move {
            let stream = match portal.monitor.receive_update_available().await {
                Ok(stream) => stream,
                Err(e) => {
                    log::warn!("cannot listen to the Flatpak update monitor: {e}");
                    return;
                }
            };
            let mut stream = std::pin::pin!(stream);
            while let Some(news) = stream.next().await {
                let found = Found {
                    running: news.running_commit().to_owned(),
                    local: news.local_commit().to_owned(),
                    remote: news.remote_commit().to_owned(),
                };
                log::info!("Flatpak update monitor: {found:?}");
                app.state::<Updater>().set_found(Some(found.clone()));
                if found.is_update() {
                    let _ = app.emit(UPDATE_EVENT, found.info(branch()));
                }
            }
        });
    }

    /// The update that the monitor found, if any.
    pub async fn check(app: &AppHandle) -> Res<Option<UpdateInfo>> {
        let updater = app.state::<Updater>();
        updater.portal(app).await?;
        Ok(updater
            .found()
            .filter(Found::is_update)
            .map(|f| f.info(branch())))
    }

    /// Install the update of this branch. `progress` gets the done part and the whole.
    pub async fn install(app: &AppHandle, progress: impl Fn(u64, u64)) -> Res<()> {
        let updater = app.state::<Updater>();
        let portal = updater.portal(app).await?;
        // Listen before the call, so no message is lost.
        let stream = portal
            .monitor
            .receive_progress()
            .await
            .map_err(portal_error)?;
        let mut stream = std::pin::pin!(stream);
        portal
            .monitor
            .update(None, UpdateOptions::default())
            .await
            .map_err(portal_error)?;
        loop {
            let next = tokio::time::timeout(PROGRESS_TIMEOUT, stream.next())
                .await
                .map_err(|_| {
                    ChordError::new("update", "The update stopped: no progress for 10 minutes.")
                })?;
            let Some(p) = next else {
                return Err(ChordError::new("update", "The Flatpak portal closed."));
            };
            match p.status() {
                Some(UpdateStatus::Done) => {
                    if let Some(mut f) = updater.found() {
                        if !f.remote.is_empty() {
                            f.local = f.remote.clone();
                        }
                        updater.set_found(Some(f));
                    }
                    return Ok(());
                }
                // Nothing to download: the new version is already installed.
                Some(UpdateStatus::Empty) => return Ok(()),
                Some(UpdateStatus::Failed) => {
                    let (code, text) =
                        failure(p.error().unwrap_or(""), p.error_message().unwrap_or(""));
                    return Err(ChordError::new(code, text));
                }
                Some(UpdateStatus::Running) | None => {
                    let (done, total) = overall_progress(p.op(), p.n_ops(), p.progress());
                    progress(done, total);
                }
            }
        }
    }

    /// Start the newest installed version of this branch, then quit. `AppHandle::restart`
    /// would start the old deployment again.
    pub async fn restart(app: &AppHandle) -> Res<()> {
        let portal = app.state::<Updater>().portal(app).await?;
        // An empty command runs the default command of the app.
        let exe = std::env::current_exe().unwrap_or_default();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        portal
            .flatpak
            .spawn(
                cwd,
                &[exe],
                HashMap::<u32, OwnedFd>::new(),
                HashMap::new(),
                SpawnFlags::LatestVersion.into(),
                SpawnOptions::default(),
            )
            .await
            .map_err(portal_error)?;
        // Free the single-instance name at once, so the new copy does not hand its start to
        // this one and quit.
        tauri_plugin_single_instance::destroy(app);
        app.exit(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: &str = "\
[Application]
name=space.foid.chord
runtime=runtime/org.gnome.Platform/x86_64/49

[Instance]
instance-id=1234567890
app-path=/var/lib/flatpak/app/space.foid.chord/x86_64/nightly/abc/files
app-commit=4e1d2a7f00c0ffee
branch=nightly
arch=x86_64
flatpak-version=1.16.1
session-bus-proxy=true

[Context]
shared=network;ipc;
";

    #[test]
    fn the_info_file_gives_the_branch_and_the_commit() {
        let info = parse_info(INFO).unwrap();
        assert_eq!(info.app_id, "space.foid.chord");
        assert_eq!(info.branch, "nightly");
        assert_eq!(info.arch.as_deref(), Some("x86_64"));
        assert_eq!(info.commit.as_deref(), Some("4e1d2a7f00c0ffee"));
        assert_eq!(info.flatpak_version.as_deref(), Some("1.16.1"));
        assert!(!info.build);
        assert!(can_update(&info));
        assert_eq!(info.switch.len(), 3);
    }

    #[test]
    fn a_key_counts_only_in_its_group() {
        let text = "[Context]\nbranch=wrong\n[Application]\nname=x.y\n# branch=no\n[Instance]\n branch = beta \n";
        assert_eq!(parse_info(text).unwrap().branch, "beta");
    }

    #[test]
    fn a_runtime_sandbox_or_an_empty_file_is_not_the_app() {
        assert_eq!(parse_info("[Runtime]\nname=org.gnome.Platform\n"), None);
        assert_eq!(parse_info(""), None);
        assert_eq!(parse_info("[Application]\nname=\n"), None);
    }

    #[test]
    fn a_build_run_does_not_update() {
        let info =
            parse_info("[Application]\nname=space.foid.chord\n[Instance]\nbuild=true\n").unwrap();
        assert!(info.build);
        assert!(!can_update(&info));
        assert_eq!(info.branch, "");
    }

    #[test]
    fn the_install_commands_name_the_remote_and_the_branch() {
        assert_eq!(
            install_command("nightly").unwrap(),
            "flatpak remote-add --if-not-exists chord-nightly \
             https://bigaouette.com/chord-nightly/flatpak/chord-nightly.flatpakrepo \
             && flatpak install chord-nightly space.foid.chord//nightly"
        );
        assert_eq!(
            install_command("beta").unwrap(),
            "flatpak remote-add --if-not-exists flathub-beta \
             https://flathub.org/beta-repo/flathub-beta.flatpakrepo \
             && flatpak install flathub-beta space.foid.chord//beta"
        );
        assert_eq!(
            install_command("stable").unwrap(),
            "flatpak install flathub space.foid.chord//stable"
        );
        assert_eq!(install_command("master"), None);
        assert_eq!(
            make_current_command("beta"),
            "flatpak make-current space.foid.chord beta"
        );
    }

    #[test]
    fn the_monitor_news_tells_an_update_from_a_restart() {
        let same = Found {
            running: "a".into(),
            local: "a".into(),
            remote: "a".into(),
        };
        assert!(!same.is_update());
        let remote = Found {
            remote: "b".into(),
            ..same.clone()
        };
        assert!(remote.is_update());
        assert!(!remote.installed());
        let local = Found {
            running: "a".into(),
            local: "b".into(),
            remote: "b".into(),
        };
        assert!(local.is_update());
        assert!(local.installed());
        let info = local.info("beta");
        assert_eq!(info.version, "");
        assert_eq!(info.channel.as_deref(), Some("beta"));
        assert_eq!(info.commit.as_deref(), Some("b"));
        assert!(info.installed);
        assert_eq!(remote.info("").channel, None);
    }

    #[test]
    fn the_progress_counts_each_operation() {
        assert_eq!(overall_progress(Some(0), Some(2), Some(50)), (50, 200));
        assert_eq!(overall_progress(Some(1), Some(2), Some(100)), (200, 200));
        // Out of range values stay inside the whole.
        assert_eq!(overall_progress(Some(5), Some(2), Some(150)), (200, 200));
        assert_eq!(overall_progress(None, None, None), (0, 100));
    }

    #[test]
    fn a_failure_gets_a_clear_text() {
        let (code, text) = failure(
            "org.freedesktop.DBus.Error.NotSupported",
            "Self update not supported, new version requires new permissions",
        );
        assert_eq!(code, "updatePermissions");
        assert_eq!(
            text,
            "This update needs new permissions. Update Chord in your software app."
        );
        assert_eq!(
            failure(
                "org.freedesktop.DBus.Error.AccessDenied",
                "Application update not allowed"
            )
            .0,
            "updateDenied"
        );
        assert_eq!(
            failure(
                "org.freedesktop.DBus.Error.Failed",
                "Unable to load summary from remote chord-nightly: Could not resolve hostname"
            )
            .0,
            "updateNetwork"
        );
        assert_eq!(
            failure(
                "org.freedesktop.Flatpak.Error.RemoteNotFound",
                "Remote 'x' not found"
            )
            .0,
            "updateNoRemote"
        );
        assert_eq!(
            failure("org.freedesktop.DBus.Error.Failed", ""),
            ("update", "The update failed.".to_owned())
        );
        assert_eq!(failure("x", "Disk full").1, "Disk full");
    }
}
