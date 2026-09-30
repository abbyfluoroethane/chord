// The cards for things that happen to a room: an invitation to it, or its end. Pure
// helpers for the store in roomalerts.svelte.ts.

export interface InviteAlert {
  kind: 'invite';
  id: string;
  room: string;
  /** The JID of the person who invites. It may have a resource. */
  from: string;
  reason: string | null;
  /** The password of a room that needs one. The invitation carries it. */
  password: string | null;
}

export interface GoneAlert {
  kind: 'gone';
  id: string;
  room: string;
  reason: string | null;
  /** A room that the owner names in place of the old one. */
  alternate: string | null;
}

export type RoomAlert = InviteAlert | GoneAlert;

/** No more than this many cards at once. The oldest go first. */
export const MAX_ALERTS = 5;

/** One card per room and kind. A new invitation to the same room replaces the old one. */
export function alertId(kind: RoomAlert['kind'], room: string): string {
  return `${kind}:${room}`;
}

/** Add a card. A card for the same room and kind is replaced and moves to the end. */
export function addAlert(list: RoomAlert[], alert: RoomAlert): RoomAlert[] {
  return [...list.filter((a) => a.id !== alert.id), alert].slice(-MAX_ALERTS);
}

export function removeAlert(list: RoomAlert[], id: string): RoomAlert[] {
  return list.filter((a) => a.id !== id);
}

/** The bare JID of an address: the part before the resource. */
export function bareOf(address: string): string {
  return address.split('/')[0];
}

/** The text of a card. */
export function alertTitle(a: RoomAlert, who: string): string {
  return a.kind === 'invite' ? `${who} invited you to a room` : 'This room was closed';
}
