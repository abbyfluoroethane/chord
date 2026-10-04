// The rules of the update setting and its texts. No state here, so they are easy to test.
// See docs/updates.md.
import type { AppInfo, BuildChannel, FlatpakInfo, FlatpakSwitch, UpdateChannel, UpdateInfo } from '$lib/chord/types';

/** The time between two automatic checks. */
export const CHECK_EVERY_MS = 24 * 60 * 60 * 1000;

/** The channels, slowest first, with the one-line hint of each. */
export const CHANNELS: readonly { value: UpdateChannel; label: string; hint: string }[] = [
  { value: 'stable', label: 'Stable', hint: 'Releases only.' },
  { value: 'beta', label: 'Beta', hint: 'Betas and releases. New features first, a few bugs.' },
  { value: 'nightly', label: 'Nightly', hint: 'A build of each day. It can break.' }
];

const RANK: Record<UpdateChannel, number> = { stable: 0, beta: 1, nightly: 2 };

/** The name of a build channel for the version line. */
export function channelLabel(c: BuildChannel): string {
  return c === 'dev' ? 'Dev build' : (CHANNELS.find((x) => x.value === c)?.label ?? c);
}

/** The channel to check: the choice, else the channel of the build. A dev build gets Stable. */
export function effectiveChannel(choice: UpdateChannel | '', build: BuildChannel): UpdateChannel {
  if (choice) return choice;
  return build === 'dev' ? 'stable' : build;
}

/**
 * The note for a channel that is slower than the build. The app never goes down a build, so
 * it keeps this one until the slower channel has a newer one.
 */
export function slowerNote(chosen: UpdateChannel, build: BuildChannel): string | null {
  if (build === 'dev' || RANK[chosen] >= RANK[build]) return null;
  return `You keep this build until ${channelLabel(chosen)} has a newer one.`;
}

/** The channel of a Flatpak branch, or null for another branch (for example `master`). */
export function branchChannel(branch: string): UpdateChannel | null {
  return CHANNELS.find((c) => c.value === branch)?.value ?? null;
}

/** The name of a Flatpak branch: the channel name, else the branch itself. */
export function branchLabel(branch: string): string {
  const c = branchChannel(branch);
  return c ? channelLabel(c) : branch || 'Unknown branch';
}

/** "Version 0.3.0-beta.2 (09c83fb) · Beta", or "· Beta (Flatpak)" in a Flatpak. */
export function versionLine(
  app: Pick<AppInfo, 'version' | 'commit' | 'channel'> & { flatpak?: Pick<FlatpakInfo, 'branch'> | null }
): string {
  const where = app.flatpak ? `${branchLabel(app.flatpak.branch)} (Flatpak)` : channelLabel(app.channel);
  return `Version ${app.version} (${app.commit}) · ${where}`;
}

/** Why this install does not update itself. */
export function offText(app: Pick<AppInfo, 'channel' | 'os' | 'flatpak'>): string {
  if (app.flatpak) return 'This Flatpak is a development run. It does not update itself.';
  if (app.channel === 'dev') return 'This is a dev build. It does not update itself.';
  if (app.os === 'linux') return 'On Linux, Chord updates itself only when it runs as a Flatpak.';
  return 'This build does not update itself.';
}

/** The commands that get the branch of a channel, in a Flatpak. */
export function switchFor(app: Pick<AppInfo, 'flatpak'> | null, channel: UpdateChannel): FlatpakSwitch | null {
  return app?.flatpak?.switch.find((s) => s.channel === channel) ?? null;
}

/** The status line of an update. A Flatpak update has no version text. */
export function updateLine(info: UpdateInfo, kind: 'available' | 'downloading' | 'ready'): string {
  const v = info.version;
  if (kind === 'available') return v ? `Version ${v} is available.` : 'A new version is available.';
  if (kind === 'downloading') return v ? `Downloading version ${v}…` : 'Downloading the update…';
  return v ? `Version ${v} is installed.` : 'A new version is ready.';
}

/** The text of the update banner. */
export function bannerText(info: UpdateInfo, kind: 'available' | 'downloading' | 'ready', pct: number | null): string {
  const v = info.version;
  if (kind === 'downloading') return `Downloading ${v ? `Chord ${v}` : 'the update'}${pct === null ? '' : ` (${pct}%)`}.`;
  const name = v ? `Chord ${v}` : 'A new version of Chord';
  return kind === 'ready' ? `${name} is ready.` : `${name} is available.`;
}

/** What tells one update from another, for a closed banner. */
export function updateKey(info: UpdateInfo): string {
  return info.version || info.commit || 'update';
}

/** Is an automatic check due? */
export function checkDue(auto: boolean, lastChecked: number | null, now: number): boolean {
  return auto && (lastChecked === null || now - lastChecked >= CHECK_EVERY_MS);
}

/** "Last checked 5 minutes ago". */
export function checkedText(lastChecked: number | null, now: number): string {
  if (lastChecked === null) return 'Not checked yet';
  const minutes = Math.floor(Math.max(0, now - lastChecked) / 60_000);
  if (minutes < 1) return 'Last checked just now';
  if (minutes < 60) return `Last checked ${minutes} ${minutes === 1 ? 'minute' : 'minutes'} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `Last checked ${hours} ${hours === 1 ? 'hour' : 'hours'} ago`;
  const days = Math.floor(hours / 24);
  return `Last checked ${days} ${days === 1 ? 'day' : 'days'} ago`;
}

/** The download percent, or null when the size is unknown. */
export function percentOf(downloaded: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.max(0, Math.floor((downloaded * 100) / total)));
}

/** "4.2 MB" */
export function megabytes(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

/** Where the updater stands. */
export type UpdateStatus =
  | { kind: 'idle' }
  | { kind: 'off' }
  | { kind: 'checking' }
  | { kind: 'current' }
  | { kind: 'available'; info: UpdateInfo }
  | { kind: 'downloading'; info: UpdateInfo; downloaded: number; total: number | null }
  | { kind: 'ready'; info: UpdateInfo }
  | { kind: 'failed'; message: string; info: UpdateInfo | null };

/** The update of a status, when it has one. */
export function infoOf(s: UpdateStatus): UpdateInfo | null {
  return s.kind === 'available' || s.kind === 'downloading' || s.kind === 'ready' ? s.info : s.kind === 'failed' ? s.info : null;
}

/** The text of an error from the bridge or anywhere else. */
export function errorText(e: unknown): string {
  if (e && typeof e === 'object' && 'message' in e && typeof e.message === 'string') return e.message;
  return String(e);
}
