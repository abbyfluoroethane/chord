// The update state for Settings → About and the banner. Call `start` once, after the prefs
// load. Nothing installs until the user clicks. In the browser preview the checks and the
// download are samples, with no Tauri call. Add `?flatpak` to the preview URL to see the
// Flatpak state. See docs/updates.md.
import type { AppInfo, UpdateChannel, UpdateInfo } from '$lib/chord/types';
import { api, live } from './bridge';
import { prefs } from './prefs.svelte';
import { branchChannel, checkDue, effectiveChannel, errorText, infoOf, updateKey, type UpdateStatus } from './updatesdata';

/** How often the timer asks whether a check is due. */
const TICK_MS = 60 * 60 * 1000;

/** The preview shows the Flatpak state with `?flatpak` in the URL. */
const previewFlatpak = typeof location !== 'undefined' && new URLSearchParams(location.search).has('flatpak');

const FLATPAK_REPO = 'https://bigaouette.com/chord-nightly/flatpak/chord-nightly.flatpakrepo';
const FLATHUB_BETA_REPO = 'https://flathub.org/beta-repo/flathub-beta.flatpakrepo';

const SAMPLE_APP: AppInfo = {
  version: '0.3.0-beta.2',
  commit: '09c83fb',
  build: 1_612_345,
  channel: 'beta',
  os: previewFlatpak ? 'linux' : 'preview',
  arch: previewFlatpak ? 'x86_64' : 'browser',
  updater: previewFlatpak ? 'flatpak' : 'tauri',
  flatpak: previewFlatpak
    ? {
        appId: 'space.foid.chord',
        branch: 'beta',
        arch: 'x86_64',
        commit: '4e1d2a7f00c0ffee',
        flatpakVersion: '1.16.1',
        switch: [
          {
            channel: 'stable',
            install: 'flatpak install flathub space.foid.chord//stable',
            makeCurrent: 'flatpak make-current space.foid.chord stable'
          },
          {
            channel: 'beta',
            install: `flatpak remote-add --if-not-exists flathub-beta ${FLATHUB_BETA_REPO} && flatpak install flathub-beta space.foid.chord//beta`,
            makeCurrent: 'flatpak make-current space.foid.chord beta'
          },
          {
            channel: 'nightly',
            install: `flatpak remote-add --if-not-exists chord-nightly ${FLATPAK_REPO} && flatpak install chord-nightly space.foid.chord//nightly`,
            makeCurrent: 'flatpak make-current space.foid.chord nightly'
          }
        ]
      }
    : null
};

const SAMPLE_UPDATE: UpdateInfo = previewFlatpak
  ? {
      version: '',
      build: 0,
      channel: 'beta',
      commit: '7c2e91b04d',
      notes: null,
      pubDate: null,
      releaseUrl: null,
      installed: false
    }
  : {
      version: '0.3.0',
      build: 1_650_002,
      channel: 'stable',
      commit: '4e1d2a7',
      notes: 'Pinned messages in rooms. Faster start. Many small fixes.',
      pubDate: '2026-10-04T06:00:00Z',
      releaseUrl: 'https://github.com/abbyfluoroethane/chord-desktop/releases',
      installed: false
    };

class Updates {
  app = $state<AppInfo | null>(null);
  status = $state<UpdateStatus>({ kind: 'idle' });
  lastChecked = $state<number | null>(null);
  /** The update whose banner the user closed (`updateKey`). */
  dismissed = $state<string | null>(null);
  private started = false;

  /** Does this install update through the Flatpak portal? */
  get flatpak(): boolean {
    return this.app?.updater === 'flatpak';
  }

  /** The channel to check. In a Flatpak, the installed branch. */
  get channel(): UpdateChannel {
    if (this.app?.flatpak) return branchChannel(this.app.flatpak.branch) ?? 'stable';
    return effectiveChannel(prefs.updateChannel, this.app?.channel ?? 'stable');
  }

  /** The update for the banner, or null. */
  get banner(): UpdateInfo | null {
    const s = this.status;
    if (s.kind !== 'available' && s.kind !== 'downloading' && s.kind !== 'ready') return null;
    return updateKey(s.info) === this.dismissed ? null : s.info;
  }

  /** Read the build, check now when automatic checks are on, and keep checking each day. */
  start(): () => void {
    if (this.started) return () => {};
    this.started = true;
    let stopListening: (() => void) | null = null;
    let stopped = false;
    void this.load().then(async () => {
      // In a Flatpak the portal looks for updates by itself and tells us.
      if (live && this.flatpak) {
        const stop = await (await api()).onUpdateAvailable((info) => this.found(info));
        if (stopped) stop();
        else stopListening = stop;
      }
      await this.maybeCheck();
    });
    const timer = setInterval(() => void this.maybeCheck(), TICK_MS);
    return () => {
      stopped = true;
      stopListening?.();
      clearInterval(timer);
      this.started = false;
    };
  }

  /** Read the version and channel of this build once. */
  async load() {
    if (this.app) return;
    try {
      this.app = live ? await (await api()).appInfo() : SAMPLE_APP;
    } catch {
      this.app = null;
    }
    if (this.app?.updater === 'none') this.status = { kind: 'off' };
    // The preview shows a check from two hours ago.
    if (!live) this.lastChecked = Date.now() - 2 * 60 * 60 * 1000;
  }

  /** The Flatpak portal found an update. Show it when automatic checks are on. */
  private found(info: UpdateInfo) {
    if (!prefs.autoUpdate) return;
    // Not while a check, a download or a failed install runs its course.
    const s = this.status;
    const free = s.kind === 'idle' || s.kind === 'current' || s.kind === 'available' || (s.kind === 'failed' && !s.info);
    if (!free) return;
    this.lastChecked = Date.now();
    this.status = info.installed ? { kind: 'ready', info } : { kind: 'available', info };
  }

  /** Check when automatic checks are on and the last check is a day old. */
  async maybeCheck() {
    if (checkDue(prefs.autoUpdate, this.lastChecked, Date.now())) await this.check();
  }

  /** Look for an update in the chosen channel. */
  async check() {
    await this.load();
    const kind = this.status.kind;
    if (kind === 'off' || kind === 'checking' || kind === 'downloading' || kind === 'ready') return;
    this.status = { kind: 'checking' };
    try {
      const info = live ? await (await api()).updateCheck(this.channel) : await sampleCheck(this.channel);
      this.lastChecked = Date.now();
      // A Flatpak that the system already updated needs only a restart.
      this.status = !info ? { kind: 'current' } : info.installed ? { kind: 'ready', info } : { kind: 'available', info };
    } catch (e) {
      this.lastChecked = Date.now();
      this.status = { kind: 'failed', message: errorText(e), info: null };
    }
  }

  /** Pick a channel and check it at once. A Flatpak changes its branch with a command instead. */
  setChannel(c: UpdateChannel) {
    if (this.app?.flatpak) return;
    prefs.set('updateChannel', c);
    if (this.status.kind === 'downloading' || this.status.kind === 'ready') return;
    this.status = { kind: 'idle' };
    void this.check();
  }

  /** Download and install the update that the last check found. */
  async install() {
    const s = this.status;
    if (s.kind !== 'available' && !(s.kind === 'failed' && s.info)) return;
    const info = infoOf(s);
    if (!info) return;
    this.status = { kind: 'downloading', info, downloaded: 0, total: null };
    const progress = (downloaded: number, total: number | null) => {
      if (this.status.kind === 'downloading') this.status = { kind: 'downloading', info, downloaded, total };
    };
    try {
      if (live) await (await api()).updateInstall((p) => progress(p.downloaded, p.total));
      else await sampleDownload(progress);
      this.status = { kind: 'ready', info };
    } catch (e) {
      this.status = { kind: 'failed', message: errorText(e), info };
    }
  }

  /** Try the step that failed again. */
  retry() {
    const s = this.status;
    if (s.kind !== 'failed') return;
    if (s.info) void this.install();
    else {
      this.status = { kind: 'idle' };
      void this.check();
    }
  }

  /** Start the app again. With an update not yet installed, install it first. */
  async restart() {
    if (this.status.kind === 'available') await this.install();
    if (this.status.kind !== 'ready') return;
    if (!live) {
      // The preview cannot restart: show the state after a restart.
      const info = this.status.info;
      if (this.app && info.version) this.app = { ...this.app, version: info.version, build: info.build };
      this.status = { kind: 'current' };
      return;
    }
    try {
      await (await api()).updateRestart();
    } catch (e) {
      this.status = { kind: 'failed', message: errorText(e), info: null };
    }
  }

  dismiss() {
    const info = this.banner;
    if (info) this.dismissed = updateKey(info);
  }
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The preview: Stable and Beta have a release, Nightly has nothing newer. */
async function sampleCheck(channel: UpdateChannel): Promise<UpdateInfo | null> {
  await wait(700);
  return channel === 'nightly' ? null : SAMPLE_UPDATE;
}

/** The preview: a 40 MB download in about two seconds. */
async function sampleDownload(progress: (downloaded: number, total: number | null) => void) {
  const total = 40_000_000;
  for (let done = 0; done < total; done += 2_000_000) {
    progress(done, total);
    await wait(100);
  }
  progress(total, total);
}

export const updates = new Updates();
