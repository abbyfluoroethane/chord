import { describe, expect, it } from 'vitest';
import type { UpdateInfo } from '$lib/chord/types';
import {
  CHECK_EVERY_MS,
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
  canInstall: true
};

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
