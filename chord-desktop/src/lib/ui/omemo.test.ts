import { describe, expect, it } from 'vitest';
import { omemo } from './omemo.svelte';

describe('omemo events', () => {
  it('keeps the devices that a send left out', () => {
    omemo.onEvent({ type: 'omemoDevicesSkipped', data: { peer: 'a@b.c', deviceIds: [7, 9] } });
    expect(omemo.skipped['a@b.c']).toEqual([7, 9]);
    omemo.dismissSkipped('a@b.c');
    expect(omemo.skipped['a@b.c']).toBeUndefined();
  });

  it('shows the failure of a change on the target message, then clears it', () => {
    omemo.onEvent({
      type: 'omemoSendFailed',
      data: { peer: 'a@b.c', messageId: 'tgt', reason: 'No key.' }
    });
    const item = { id: 'm:1', originId: 'tgt', stanzaId: null };
    expect(omemo.failureOf(item)).toBe('No key.');
    omemo.clearFailure(item);
    expect(omemo.failureOf(item)).toBeNull();
  });
});
