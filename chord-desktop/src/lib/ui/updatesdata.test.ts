import { describe, expect, it } from 'vitest';
import type { FlatpakSwitch, UpdateInfo } from '$lib/chord/types';
import {
  CHECK_EVERY_MS,
  bannerText,
  branchChannel,
  branchLabel,
  offText,
  switchFor,
  updateKey,
  updateLine,
  channelLabel,
  checkDue,
  checkedText,
  effectiveChannel,
  errorText,
  infoOf,
  megabytes,
  percentOf,
  slowerNote,
  versionLine
} from './updatesdata';

const INFO: UpdateInfo = {
  version: '0.3.0',
  build: 10,
  channel: 'stable',
  commit: 'abc1234',
  notes: null,
  pubDate: null,
  releaseUrl: null,
  installed: false
};

const FLATPAK_UPDATE: UpdateInfo = { ...INFO, version: '', build: 0, commit: '7c2e91b04d' };

const SWITCH: FlatpakSwitch = {
  channel: 'nightly',
  install: 'flatpak install chord-nightly space.foid.chord//nightly',
  makeCurrent: 'flatpak make-current space.foid.chord nightly'
};

describe('flatpak', () => {
  it('reads the channel from the branch', () => {
    expect(branchChannel('beta')).toBe('beta');
    expect(branchChannel('master')).toBeNull();
    expect(branchLabel('nightly')).toBe('Nightly');
    expect(branchLabel('master')).toBe('master');
    expect(branchLabel('')).toBe('Unknown branch');
  });
  it('names the branch in the version line', () => {
    expect(versionLine({ version: '0.3.0', commit: 'abc', channel: 'beta', flatpak: { branch: 'beta' } })).toBe(
      'Version 0.3.0 (abc) · Beta (Flatpak)'
    );
  });
  it('finds the switch command of a channel', () => {
    expect(switchFor({ flatpak: { appId: 'a', branch: 'beta', arch: null, commit: null, flatpakVersion: null, switch: [SWITCH] } }, 'nightly')).toBe(SWITCH);
    expect(switchFor({ flatpak: null }, 'nightly')).toBeNull();
    expect(switchFor(null, 'nightly')).toBeNull();
  });
  it('says why an install has no updater', () => {
    const fp = { appId: 'a', branch: '', arch: null, commit: null, flatpakVersion: null, switch: [] };
    expect(offText({ channel: 'beta', os: 'linux', flatpak: fp })).toContain('development run');
    expect(offText({ channel: 'dev', os: 'linux', flatpak: null })).toBe('This is a dev build. It does not update itself.');
    expect(offText({ channel: 'beta', os: 'linux', flatpak: null })).toBe(
      'On Linux, Chord updates itself only when it runs as a Flatpak.'
    );
  });
});

describe('update texts', () => {
  it('use the version when there is one', () => {
    expect(updateLine(INFO, 'available')).toBe('Version 0.3.0 is available.');
    expect(updateLine(INFO, 'downloading')).toBe('Downloading version 0.3.0…');
    expect(updateLine(INFO, 'ready')).toBe('Version 0.3.0 is installed.');
    expect(bannerText(INFO, 'available', null)).toBe('Chord 0.3.0 is available.');
    expect(bannerText(INFO, 'downloading', 40)).toBe('Downloading Chord 0.3.0 (40%).');
    expect(bannerText(INFO, 'ready', null)).toBe('Chord 0.3.0 is ready.');
    expect(updateKey(INFO)).toBe('0.3.0');
  });
  it('speak of a new version for a Flatpak update', () => {
    expect(updateLine(FLATPAK_UPDATE, 'available')).toBe('A new version is available.');
    expect(updateLine(FLATPAK_UPDATE, 'downloading')).toBe('Downloading the update…');
    expect(updateLine(FLATPAK_UPDATE, 'ready')).toBe('A new version is ready.');
    expect(bannerText(FLATPAK_UPDATE, 'available', null)).toBe('A new version of Chord is available.');
    expect(bannerText(FLATPAK_UPDATE, 'downloading', null)).toBe('Downloading the update.');
    expect(bannerText(FLATPAK_UPDATE, 'ready', null)).toBe('A new version of Chord is ready.');
    expect(updateKey(FLATPAK_UPDATE)).toBe('7c2e91b04d');
  });
});

describe('effectiveChannel', () => {
  it('follows the build until the user picks one', () => {
    expect(effectiveChannel('', 'beta')).toBe('beta');
    expect(effectiveChannel('', 'nightly')).toBe('nightly');
    expect(effectiveChannel('nightly', 'stable')).toBe('nightly');
  });
  it('gives a dev build Stable', () => {
    expect(effectiveChannel('', 'dev')).toBe('stable');
  });
});

describe('slowerNote', () => {
  it('speaks only for a channel slower than the build', () => {
    expect(slowerNote('stable', 'nightly')).toBe('You keep this build until Stable has a newer one.');
    expect(slowerNote('beta', 'nightly')).toBe('You keep this build until Beta has a newer one.');
    expect(slowerNote('nightly', 'beta')).toBeNull();
    expect(slowerNote('beta', 'beta')).toBeNull();
    expect(slowerNote('stable', 'dev')).toBeNull();
  });
});

describe('versionLine', () => {
  it('names the version, the commit and the channel', () => {
    expect(versionLine({ version: '0.3.0-beta.2', commit: '09c83fb', channel: 'beta' })).toBe(
      'Version 0.3.0-beta.2 (09c83fb) · Beta'
    );
    expect(channelLabel('dev')).toBe('Dev build');
  });
});

describe('checkDue', () => {
  it('checks at start and then once a day, only when on', () => {
    expect(checkDue(true, null, 1000)).toBe(true);
    expect(checkDue(false, null, 1000)).toBe(false);
    expect(checkDue(true, 0, CHECK_EVERY_MS - 1)).toBe(false);
    expect(checkDue(true, 0, CHECK_EVERY_MS)).toBe(true);
  });
});

describe('checkedText', () => {
  const now = 10 * CHECK_EVERY_MS;
  it('says how long ago', () => {
    expect(checkedText(null, now)).toBe('Not checked yet');
    expect(checkedText(now - 10_000, now)).toBe('Last checked just now');
    expect(checkedText(now - 60_000, now)).toBe('Last checked 1 minute ago');
    expect(checkedText(now - 5 * 60_000, now)).toBe('Last checked 5 minutes ago');
    expect(checkedText(now - 2 * 3_600_000, now)).toBe('Last checked 2 hours ago');
    expect(checkedText(now - 3 * CHECK_EVERY_MS, now)).toBe('Last checked 3 days ago');
  });
});

describe('progress', () => {
  it('gives a percent only with a size', () => {
    expect(percentOf(50, 200)).toBe(25);
    expect(percentOf(300, 200)).toBe(100);
    expect(percentOf(50, null)).toBeNull();
    expect(percentOf(50, 0)).toBeNull();
    expect(megabytes(4_200_000)).toBe('4.2 MB');
  });
});

describe('infoOf', () => {
  it('finds the update of a status', () => {
    expect(infoOf({ kind: 'available', info: INFO })).toBe(INFO);
    expect(infoOf({ kind: 'downloading', info: INFO, downloaded: 1, total: null })).toBe(INFO);
    expect(infoOf({ kind: 'failed', message: 'x', info: null })).toBeNull();
    expect(infoOf({ kind: 'current' })).toBeNull();
  });
});

describe('errorText', () => {
  it('reads a bridge error or anything else', () => {
    expect(errorText({ code: 'update', message: 'no network' })).toBe('no network');
    expect(errorText(new Error('bad'))).toBe('bad');
    expect(errorText('plain')).toBe('plain');
  });
});
