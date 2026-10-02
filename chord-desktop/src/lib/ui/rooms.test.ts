import { describe, expect, it } from 'vitest';
import { canModerateMember, canSetTopic, groupMembers, isModerator } from './rooms';
import type { Affiliation, MemberItem } from './types';

function member(id: string, over: Partial<MemberItem> = {}): MemberItem {
  return {
    id,
    name: id,
    role: null,
    affiliation: 'none' as Affiliation,
    show: null,
    status: null,
    online: true,
    avatar: null,
    ...over
  };
}

describe('groupMembers', () => {
  it('puts visitors in their own group, after the online people', () => {
    const groups = groupMembers([
      member('own', { affiliation: 'owner', role: 'Moderator' }),
      member('adm', { affiliation: 'admin', role: 'Moderator' }),
      member('bob'),
      member('mute', { role: 'Visitor', affiliation: 'member' }),
      member('gone', { online: false })
    ]);
    expect(groups.map((g) => [g.label, g.items.map((m) => m.id)])).toEqual([
      ['Owners', ['own']],
      ['Admins', ['adm']],
      ['Online', ['bob']],
      ['Visitors', ['mute']],
      ['Offline', ['gone']]
    ]);
  });

  it('leaves out an empty group', () => {
    expect(groupMembers([member('a')]).map((g) => g.label)).toEqual(['Online']);
    expect(groupMembers([])).toEqual([]);
  });

  it('keeps an admin who is a visitor in the admin group', () => {
    const groups = groupMembers([member('adm', { affiliation: 'admin', role: 'Visitor' })]);
    expect(groups.map((g) => g.label)).toEqual(['Admins']);
  });
});

describe('who may do what', () => {
  const mod = member('mod', { role: 'Moderator' });
  const owner = member('own', { affiliation: 'owner', role: 'Moderator' });
  const admin = member('adm', { affiliation: 'admin', role: 'Moderator' });
  const plain = member('bob', { role: 'Participant' });

  it('knows a moderator', () => {
    expect(isModerator(mod)).toBe(true);
    expect(isModerator(admin)).toBe(true);
    expect(isModerator(plain)).toBe(false);
    expect(isModerator(undefined)).toBe(false);
  });

  it('lets a moderator set the topic, and anyone when the room allows it', () => {
    expect(canSetTopic(mod, false)).toBe(true);
    expect(canSetTopic(plain, false)).toBe(false);
    expect(canSetTopic(plain, true)).toBe(true);
    expect(canSetTopic(member('v', { role: 'Visitor' }), true)).toBe(false);
    expect(canSetTopic(undefined, true)).toBe(false);
  });

  it('lets a moderator kick a plain member and not an admin or an owner', () => {
    expect(canModerateMember(mod, plain)).toBe(true);
    expect(canModerateMember(mod, admin)).toBe(false);
    expect(canModerateMember(mod, owner)).toBe(false);
    expect(canModerateMember(admin, owner)).toBe(false);
    expect(canModerateMember(owner, admin)).toBe(true);
  });

  it('never lets anyone act on himself or act without the right', () => {
    expect(canModerateMember(mod, mod)).toBe(false);
    expect(canModerateMember(plain, member('x'))).toBe(false);
    expect(canModerateMember(mod, undefined)).toBe(false);
    expect(canModerateMember(undefined, plain)).toBe(false);
  });
});
