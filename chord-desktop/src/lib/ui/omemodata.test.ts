import { describe, expect, it } from 'vitest';
import type { OmemoDevice } from '$lib/chord/types';
import {
  UPLOAD_HINT,
  canEncrypt,
  deviceName,
  fingerprintBlocks,
  fingerprintText,
  isBlocked,
  lockKind,
  skippedText,
  UNTRUSTED_HINT,
  lockView,
  normalizeFingerprint,
  sendFailedText,
  sortDevices,
  trustButtons,
  trustFor,
  trustLabel,
  uploadState
} from './omemodata';

const HEX = '0123456789abcdef'.repeat(4);

function dev(over: Partial<OmemoDevice> = {}): OmemoDevice {
  return {
    jid: 'rin@foid.space',
    deviceId: 1,
    fingerprint: HEX,
    label: null,
    trust: 'blind',
    active: true,
    own: false,
    thisDevice: false,
    ...over
  };
}

describe('fingerprintBlocks', () => {
  it('makes 8 blocks of 8 characters', () => {
    const blocks = fingerprintBlocks(HEX);
    expect(blocks).toHaveLength(8);
    expect(blocks.every((b) => b.length === 8)).toBe(true);
    expect(blocks.join('')).toBe(HEX);
  });

  it('accepts text that has spaces and upper case', () => {
    const spaced = fingerprintText(HEX.toUpperCase());
    expect(spaced).toBe(fingerprintBlocks(HEX).join(' '));
    expect(fingerprintBlocks(spaced)).toEqual(fingerprintBlocks(HEX));
  });

  it('gives no block for a missing key', () => {
    expect(fingerprintBlocks(null)).toEqual([]);
    expect(fingerprintBlocks('')).toEqual([]);
    expect(fingerprintText(undefined)).toBe('');
  });

  it('keeps a short last block', () => {
    expect(fingerprintBlocks('aabbccddeeff')).toEqual(['aabbccdd', 'eeff']);
  });

  it('removes colons and dashes', () => {
    expect(normalizeFingerprint('AA:BB-cc dd')).toBe('aabbccdd');
  });
});

describe('trust buttons', () => {
  it('lets a trusted device be verified or distrusted', () => {
    expect(trustButtons(dev({ trust: 'blind' }))).toEqual({ verify: true, distrust: true });
  });

  it('turns Verify off for a verified device', () => {
    expect(trustButtons(dev({ trust: 'verified' }))).toEqual({ verify: false, distrust: true });
  });

  it('turns Distrust off for a distrusted device', () => {
    expect(trustButtons(dev({ trust: 'distrusted' }))).toEqual({ verify: true, distrust: false });
  });

  it('lets the user decide on an undecided device', () => {
    expect(trustButtons(dev({ trust: 'undecided' }))).toEqual({ verify: true, distrust: true });
  });

  it('turns both off when there is no key', () => {
    expect(trustButtons(dev({ trust: null, fingerprint: null }))).toEqual({
      verify: false,
      distrust: false
    });
  });

  it('maps each button to a trust level', () => {
    expect(trustFor('verify')).toBe('verified');
    expect(trustFor('distrust')).toBe('distrusted');
  });

  it('names the trust levels', () => {
    expect(trustLabel('verified')).toBe('Verified');
    expect(trustLabel(null)).toBe('No key yet');
    expect(isBlocked('undecided')).toBe(true);
    expect(isBlocked('distrusted')).toBe(true);
    expect(isBlocked('blind')).toBe(false);
    expect(isBlocked(null)).toBe(false);
  });
});

describe('the lock toggle', () => {
  it('shows only in a 1:1 chat', () => {
    expect(canEncrypt({ kind: 'dm' })).toBe(true);
    expect(canEncrypt({ kind: 'channel' })).toBe(false);
    expect(canEncrypt({ kind: 'dm', pm: { room: 'r', nick: 'n' } })).toBe(false);
    expect(canEncrypt(null)).toBe(false);
  });

  it('is off by default and says what a click does', () => {
    expect(lockView(true, false)).toEqual({
      show: true,
      on: false,
      label: 'Turn on encryption'
    });
    expect(lockView(true, true).label).toBe('Turn off encryption');
    expect(lockView(false, false).show).toBe(false);
  });

  it('turns the upload button off in an encrypted chat', () => {
    expect(uploadState(true)).toEqual({ disabled: true, hint: UPLOAD_HINT });
    expect(uploadState(false).disabled).toBe(false);
  });
});

describe('devices and failures', () => {
  it('puts active devices first', () => {
    const list = sortDevices([
      dev({ deviceId: 3, active: false }),
      dev({ deviceId: 9 }),
      dev({ deviceId: 2 })
    ]);
    expect(list.map((d) => d.deviceId)).toEqual([2, 9, 3]);
  });

  it('names a device by label or id', () => {
    expect(deviceName({ label: ' Phone ', deviceId: 4 })).toBe('Phone');
    expect(deviceName({ label: null, deviceId: 4 })).toBe('Device 4');
  });

  it('shows the reason of a failed send', () => {
    expect(sendFailedText('No key for this contact.')).toBe('Not sent. No key for this contact.');
    expect(sendFailedText(undefined)).toBe('Not sent.');
  });
});

describe('untrusted messages and skipped devices', () => {
  it('picks the lock of a message', () => {
    expect(lockKind('omemo')).toBe('ok');
    expect(lockKind('omemo-untrusted')).toBe('warn');
    expect(lockKind('undecryptable')).toBe('none');
    expect(lockKind(undefined)).toBe('none');
    expect(UNTRUSTED_HINT).toBe('From an unverified device.');
  });

  it('counts the skipped devices', () => {
    expect(skippedText(1)).toBe('Not sent to 1 device.');
    expect(skippedText(2)).toBe('Not sent to 2 devices.');
  });
});
