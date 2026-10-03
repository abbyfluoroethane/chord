import { describe, expect, it } from 'vitest';
import { debugText, featureName, knownFeatures, uptimeText } from './advanceddata';

describe('featureName', () => {
  it('names a feature with or without its version', () => {
    expect(featureName('urn:xmpp:mam:2')).toBe('Message archive');
    expect(featureName('urn:xmpp:mam:1')).toBe('Message archive');
    expect(featureName('urn:xmpp:http:upload:0')).toBe('File upload');
    expect(featureName('http://jabber.org/protocol/muc')).toBe('Group chat');
  });
  it('does not name a feature it does not know', () => {
    expect(featureName('urn:example:unknown:1')).toBeNull();
  });
  it('names a sub feature after its parent', () => {
    expect(featureName('http://jabber.org/protocol/muc#stable_id')).toBe('Group chat');
  });
});

describe('knownFeatures', () => {
  it('lists each name once, in page order', () => {
    const list = ['urn:xmpp:push:0', 'urn:xmpp:avatar:data', 'urn:xmpp:avatar:metadata', 'urn:xmpp:mam:2', 'x:y'];
    expect(knownFeatures(list)).toEqual(['Message archive', 'Push notifications', 'Avatars']);
  });
  it('gives an empty list for an empty list', () => {
    expect(knownFeatures([])).toEqual([]);
  });
});

describe('uptimeText', () => {
  it('counts in the largest units', () => {
    expect(uptimeText(10_000)).toBe('Under a minute');
    expect(uptimeText(60_000)).toBe('1 minute');
    expect(uptimeText(5 * 60_000)).toBe('5 minutes');
    expect(uptimeText(60 * 60_000)).toBe('1 hour');
    expect(uptimeText(125 * 60_000)).toBe('2 hours 5 minutes');
    expect(uptimeText(27 * 60 * 60_000)).toBe('1 day 3 hours');
    expect(uptimeText(-5)).toBe('Under a minute');
  });
});

describe('debugText', () => {
  it('has the version, the system, the server, and the features', () => {
    const text = debugText({
      version: '0.1.0',
      os: 'macos',
      arch: 'aarch64',
      server: 'example.org',
      status: 'Connected',
      features: ['a', 'b']
    });
    expect(text.split('\n')).toEqual([
      'Chord 0.1.0',
      'System: macos aarch64',
      'Server: example.org',
      'Connection: Connected',
      'Server features (2): a, b'
    ]);
  });
  it('says none when there is nothing', () => {
    const text = debugText({ version: '1', os: 'o', arch: 'a', server: '', status: 'Offline', features: [] });
    expect(text).toContain('Server: none');
    expect(text).toContain('Server features (0): none');
  });
});
