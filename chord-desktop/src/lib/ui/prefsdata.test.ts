import { describe, expect, it } from 'vitest';
import { CHAT_DEFAULTS, parseLegacy, parsePrefs, resolvePrefs } from './prefsdata';

describe('parsePrefs', () => {
  it('keeps good fields and drops bad ones', () => {
    expect(
      parsePrefs({ sound: false, muteDms: 'yes', display: 'compact', emojiPack: 'nope', fontSize: 99 })
    ).toEqual({ sound: false, display: 'compact', fontSize: 20 });
  });
  it('reads the shareInfo switch as a boolean only', () => {
    expect(parsePrefs({ shareInfo: false })).toEqual({ shareInfo: false });
    expect(parsePrefs({ shareInfo: 'off' })).toEqual({});
  });
  it('takes no prefs from a non-object', () => {
    expect(parsePrefs(null)).toEqual({});
    expect(parsePrefs('x')).toEqual({});
  });
});

describe('resolvePrefs', () => {
  const legacy = JSON.stringify({ sound: false, fontSize: 18, desktopNotifications: false });

  it('uses the file and does not migrate when the old copy adds nothing', () => {
    const r = resolvePrefs({ sound: true }, JSON.stringify({ sound: false }));
    expect(r).toEqual({ prefs: { sound: true }, migrate: false });
  });
  it('migrates the old copy when the file has no prefs', () => {
    const r = resolvePrefs(undefined, legacy);
    expect(r.migrate).toBe(true);
    expect(r.prefs).toEqual({ sound: false, fontSize: 18, desktopNotifications: false });
  });
  it('fills the fields the file lacks from the old copy', () => {
    const r = resolvePrefs({ sound: true }, legacy);
    expect(r.prefs).toEqual({ sound: true, fontSize: 18, desktopNotifications: false });
    expect(r.migrate).toBe(true);
  });
  it('ignores a damaged old copy', () => {
    expect(parseLegacy('{oops')).toEqual({});
    expect(resolvePrefs(undefined, '{oops')).toEqual({ prefs: {}, migrate: false });
    expect(resolvePrefs(undefined, null)).toEqual({ prefs: {}, migrate: false });
  });
});

describe('Chat prefs', () => {
  it('has defaults that keep the old behaviour', () => {
    expect(CHAT_DEFAULTS).toEqual({
      sendKey: 'enter',
      inlineMedia: true,
      gifs: 'always',
      autoplayVideo: false,
      showSpoilers: false,
      emoticons: false,
      spellcheck: true,
      confirmDelete: true
    });
  });
  it('keeps good Chat values', () => {
    const v = {
      sendKey: 'mod-enter',
      inlineMedia: false,
      gifs: 'hover',
      autoplayVideo: true,
      showSpoilers: true,
      emoticons: true,
      spellcheck: false,
      confirmDelete: false
    };
    expect(parsePrefs(v)).toEqual(v);
  });
  it('drops bad Chat values', () => {
    expect(
      parsePrefs({ sendKey: 'tab', inlineMedia: 1, gifs: 'never', autoplayVideo: 'yes', confirmDelete: null })
    ).toEqual({});
  });
});
