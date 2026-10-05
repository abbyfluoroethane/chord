// Pure logic for OMEMO in the UI: fingerprint text, trust labels and buttons, the lock.
// The store (omemo.svelte.ts) and the components call these functions.
import type { OmemoDevice, OmemoTrust } from '$lib/chord/types';

/** Text of the caption under the upload button when the chat is encrypted. */
export const UPLOAD_HINT = 'Files are not sent in an encrypted chat yet.';

/** Remove spaces, colons and dashes. Lower case. */
export function normalizeFingerprint(fp: string): string {
  return fp.replace(/[\s:-]/g, '').toLowerCase();
}

/** Split a fingerprint into blocks of 8 hex characters. A missing fingerprint gives no block. */
export function fingerprintBlocks(fp: string | null | undefined): string[] {
  if (!fp) return [];
  const flat = normalizeFingerprint(fp);
  const out: string[] = [];
  for (let i = 0; i < flat.length; i += 8) out.push(flat.slice(i, i + 8));
  return out;
}

/** The text that the copy button puts in the clipboard: blocks with one space between. */
export function fingerprintText(fp: string | null | undefined): string {
  return fingerprintBlocks(fp).join(' ');
}

/** Short words for the trust of a key. A key that we have not seen has no trust. */
export function trustLabel(trust: OmemoTrust | null): string {
  switch (trust) {
    case 'verified':
      return 'Verified';
    case 'blind':
      return 'Trusted, not verified';
    case 'distrusted':
      return 'Not trusted';
    case 'undecided':
      return 'Needs a decision';
    default:
      return 'No key yet';
  }
}

/** True when the device gets no message keys. */
export function isBlocked(trust: OmemoTrust | null): boolean {
  return trust === 'undecided' || trust === 'distrusted';
}

export interface TrustButtons {
  /** The Verify button works. */
  verify: boolean;
  /** The Distrust button works. */
  distrust: boolean;
}

/** Which trust buttons work for a device. Without a key, both stay off. */
export function trustButtons(d: Pick<OmemoDevice, 'trust' | 'fingerprint'>): TrustButtons {
  if (!d.fingerprint || d.trust === null) return { verify: false, distrust: false };
  return { verify: d.trust !== 'verified', distrust: d.trust !== 'distrusted' };
}

/** The trust that a button sets. */
export function trustFor(action: 'verify' | 'distrust'): OmemoTrust {
  return action === 'verify' ? 'verified' : 'distrusted';
}

/** Devices in the order of the dialog: active first, then by id. */
export function sortDevices(list: OmemoDevice[]): OmemoDevice[] {
  return [...list].sort((a, b) => Number(b.active) - Number(a.active) || a.deviceId - b.deviceId);
}

/** The name of a device: its label, or its id. */
export function deviceName(d: Pick<OmemoDevice, 'label' | 'deviceId'>): string {
  return d.label?.trim() || `Device ${d.deviceId}`;
}

/** Only a 1:1 chat can use OMEMO. A room and a private room chat cannot. */
export function canEncrypt(c: { kind: 'channel' | 'dm'; pm?: unknown } | null | undefined): boolean {
  return !!c && c.kind === 'dm' && !c.pm;
}

export interface LockView {
  /** Show the toggle in the composer. */
  show: boolean;
  on: boolean;
  label: string;
}

/** What the lock toggle shows. */
export function lockView(canUse: boolean, on: boolean): LockView {
  return {
    show: canUse,
    on,
    label: on ? 'Turn off encryption' : 'Turn on encryption'
  };
}

/** The upload button: off, with a one line reason, while the chat is encrypted. */
export function uploadState(encrypted: boolean): { disabled: boolean; hint: string } {
  return encrypted
    ? { disabled: true, hint: UPLOAD_HINT }
    : { disabled: false, hint: 'Upload a file' };
}

/** The text under a message that an encrypted chat did not send. */
export function sendFailedText(reason: string | undefined): string {
  const r = reason?.trim();
  return r ? `Not sent. ${r}` : 'Not sent.';
}

/** The caption of the notice about a new key. */
export function trustNeededText(name: string): string {
  return `${name} has a new key.`;
}
