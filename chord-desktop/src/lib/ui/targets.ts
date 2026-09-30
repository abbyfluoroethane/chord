// The places that the quick switcher and the forward dialog list: spaces, channels, DMs.
import { app } from './app.svelte';
import { spaceKey } from './types';

export interface Target {
  id: string;
  label: string;
  hint: string;
  kind: 'circle' | 'channel' | 'dm';
  /** The chat address. Not set for a space. */
  jid?: string;
  /** The space key. Set for a space. */
  space?: string;
}

export function switcherTargets(): Target[] {
  return [
    ...app.spaces.map((s) => ({
      id: `s:${spaceKey(s)}`,
      label: s.name,
      hint: 'Space',
      kind: 'circle' as const,
      space: spaceKey(s)
    })),
    ...app.channels.map((c) => ({
      id: `c:${c.jid}`,
      label: c.name,
      hint: c.kind === 'dm' ? 'Direct message' : (app.spaceOf(c.space ?? '')?.name ?? ''),
      kind: c.kind === 'dm' ? ('dm' as const) : ('channel' as const),
      jid: c.jid
    }))
  ];
}
