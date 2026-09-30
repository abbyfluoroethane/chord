import { describe, expect, it } from 'vitest';
import {
  MAX_ALERTS,
  addAlert,
  alertId,
  alertTitle,
  bareOf,
  removeAlert,
  type GoneAlert,
  type InviteAlert
} from './roomalerts';

const invite = (room: string, from = 'bob@example.org/phone'): InviteAlert => ({
  kind: 'invite',
  id: alertId('invite', room),
  room,
  from,
  reason: null,
  password: null
});

const gone = (room: string): GoneAlert => ({
  kind: 'gone',
  id: alertId('gone', room),
  room,
  reason: 'Moved',
  alternate: 'new@rooms.example.org'
});

describe('room alerts', () => {
  it('keeps one card per room and kind, and the newest one wins', () => {
    const first = invite('a@rooms.example.org');
    const second = { ...invite('a@rooms.example.org', 'carol@example.org'), reason: 'again' };
    const list = addAlert(addAlert([], first), second);
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ from: 'carol@example.org', reason: 'again' });
  });

  it('keeps an invitation and the end of the same room apart', () => {
    const list = addAlert(addAlert([], invite('a@r.org')), gone('a@r.org'));
    expect(list.map((a) => a.kind)).toEqual(['invite', 'gone']);
  });

  it('drops the oldest card past the limit', () => {
    let list: ReturnType<typeof addAlert> = [];
    for (let i = 0; i < MAX_ALERTS + 2; i += 1) list = addAlert(list, invite(`r${i}@r.org`));
    expect(list).toHaveLength(MAX_ALERTS);
    expect(list[0].room).toBe('r2@r.org');
  });

  it('removes a card by id', () => {
    const list = addAlert(addAlert([], invite('a@r.org')), invite('b@r.org'));
    expect(removeAlert(list, alertId('invite', 'a@r.org')).map((a) => a.room)).toEqual(['b@r.org']);
    expect(removeAlert(list, 'nothing')).toHaveLength(2);
  });

  it('strips the resource and words the titles', () => {
    expect(bareOf('bob@example.org/phone')).toBe('bob@example.org');
    expect(bareOf('bob@example.org')).toBe('bob@example.org');
    expect(alertTitle(invite('a@r.org'), 'Bob')).toBe('Bob invited you to a room');
    expect(alertTitle(gone('a@r.org'), 'Bob')).toBe('This room was closed');
  });
});
