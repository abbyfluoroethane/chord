// Cards for what happens to a room: an invitation to it (XEP-0045 mediated or XEP-0249
// direct), and its end. An invitation never joins by itself: the user clicks Accept or
// Decline. Accept joins with the password of the invitation. Decline sends the decline to
// the room (a mediated decline), which tells the person who invited.
import { plainError } from './adapt';
import { api, live } from './bridge';
import { app } from './app.svelte';
import {
  addAlert,
  alertId,
  bareOf,
  removeAlert,
  type GoneAlert,
  type InviteAlert,
  type RoomAlert
} from './roomalerts';
import { ui } from './ui.svelte';
import { xmppLinks } from './xmpplinks.svelte';

class RoomAlerts {
  list = $state<RoomAlert[]>([]);

  /** An invitation came in. A room that we are in already needs no card. */
  invited(room: string, from: string, reason: string | null, password: string | null) {
    if (app.channels.some((c) => c.jid === room && c.kind === 'channel' && c.joined)) return;
    const card: InviteAlert = {
      kind: 'invite',
      id: alertId('invite', room),
      room,
      from,
      reason,
      password
    };
    this.list = addAlert(this.list, card);
    // The card shows the name of the room and how many people are in it.
    xmppLinks.request({ kind: 'room', jid: room, password: null });
  }

  /** The room was destroyed. Say why, and offer the room that the owner names. */
  destroyed(room: string, reason: string | null, alternate: string | null) {
    app.markRoomGone(room);
    const card: GoneAlert = { kind: 'gone', id: alertId('gone', room), room, reason, alternate };
    // An invitation to a room that is gone is of no use.
    this.list = addAlert(removeAlert(this.list, alertId('invite', room)), card);
  }

  dismiss(id: string) {
    this.list = removeAlert(this.list, id);
  }

  /** Join the room of an invitation. The card goes when the join works. */
  async accept(card: InviteAlert): Promise<void> {
    if (await app.acceptRoomInvite(card.room, card.password)) this.dismiss(card.id);
  }

  /** Send the decline to the room, and drop the card. */
  async decline(card: InviteAlert): Promise<void> {
    this.dismiss(card.id);
    if (!live) return;
    try {
      const b = await api();
      await b.declineRoomInvite(card.room, bareOf(card.from));
    } catch (e) {
      ui.say(plainError(e));
    }
  }

  /** Go to the room that the owner names in place of a destroyed one. */
  async goToAlternate(card: GoneAlert): Promise<void> {
    if (card.alternate && (await app.joinRoomLink(card.alternate, null))) this.dismiss(card.id);
  }
}

export const roomAlerts = new RoomAlerts();
