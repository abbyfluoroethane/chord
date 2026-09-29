// The places that the quick switcher and the forward dialog list: circles, channels, DMs.
import { app } from './app.svelte';
import { spaceKey } from './types';

export interface Target {
  id: string;
  label: string;
  hint: string;
  kind: 'circle' | 'channel' | 'dm';
  /** The chat address. Not set for a circle. */
  jid?: string;
  /** The circle key. Set for a circle. */
  space?: string;
}

export function switcherTargets(): Target[] {
  return [
    ...app.spaces.map((s) => ({
      id: `s:${spaceKey(s)}`,
      label: s.name,
      hint: 'Circle',
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
