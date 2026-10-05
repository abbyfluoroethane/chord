// Sample OMEMO data for the browser preview. There is no core: these values only show the UI.
import type { OmemoDevice } from '$lib/chord/types';

/** The chat that starts with encryption on. */
export const encryptedChats = ['sam@other.example'];

/** Why a sample message failed, by message id. */
export const failures: Record<string, string> = {
  'sam-fail': 'Sam has no device that you trust.'
};

export const devices: Record<string, OmemoDevice[]> = {
  'sam@other.example': [
    {
      jid: 'sam@other.example',
      deviceId: 1084213,
      fingerprint: '5f3a91c2d8e4170b6a2c9e55f01d83b7c4a6e290138bd75e0c9f4a21b86d3e07',
      label: 'Phone',
      trust: 'verified',
      active: true,
      own: false,
      thisDevice: false
    },
    {
      jid: 'sam@other.example',
      deviceId: 772901,
      fingerprint: 'a07be41c93d25f6808e1b3c7d49a2f50e6c81d3b7a94f205c18e6d0b3f47a9c2',
      label: 'Laptop',
      trust: 'undecided',
      active: true,
      own: false,
      thisDevice: false
    },
    {
      jid: 'sam@other.example',
      deviceId: 330518,
      fingerprint: '19cd0e7a4b83f2561d9ae8c0735b4f1e2a6d90c85e3b17f4a02d6c98b1e5f37a',
      label: null,
      trust: 'distrusted',
      active: false,
      own: false,
      thisDevice: false
    }
  ],
  'rin@foid.space': [
    {
      jid: 'rin@foid.space',
      deviceId: 4410276,
      fingerprint: 'c3e90a5d17b84f2e6a01d9c8b5734e2f0a9d16c7e4b82f503d1a6e97c0b4f825',
      label: 'Desktop',
      trust: 'blind',
      active: true,
      own: false,
      thisDevice: false
    }
  ]
};

export const ownDevices: OmemoDevice[] = [
  {
    jid: 'abby@foid.space',
    deviceId: 90210553,
    fingerprint: '7d41e8b3a09c562f1e3d7a84c0b95f26d8e1a374c9f05b2e6a8d13740fc5e9b1',
    label: 'Chord on this computer',
    trust: 'verified',
    active: true,
    own: true,
    thisDevice: true
  },
  {
    jid: 'abby@foid.space',
    deviceId: 5512047,
    fingerprint: 'e2b6f0c4a7d18359b04e6c92f1a73d58c0e49b7a2d6f1835e90c4b7a1d2f68ea',
    label: 'Phone',
    trust: 'blind',
    active: true,
    own: true,
    thisDevice: false
  },
  {
    jid: 'abby@foid.space',
    deviceId: 2260814,
    fingerprint: '0b95d3e7c1a4f86285e0b7d39c4a1f6e8d2053b97c4e1a6f30d85b2e9c7a4f10',
    label: 'Old tablet',
    trust: 'blind',
    active: true,
    own: true,
    thisDevice: false
  }
];
