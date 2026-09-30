// Pure helpers for the people of a room: the groups of the member list, and who may do
// what. The room has the last word: these decide only what the UI offers.
import type { Affiliation, MemberItem } from './types';

export interface MemberGroup {
  label: string;
  items: MemberItem[];
}

/** The groups of the member list: owners, admins, online, visitors, offline. */
export function groupMembers(members: MemberItem[]): MemberGroup[] {
  const on = members.filter((m) => m.online);
  const visitor = (m: MemberItem) => m.role === 'Visitor';
  const staff = (m: MemberItem) => m.affiliation === 'owner' || m.affiliation === 'admin';
  const groups: MemberGroup[] = [
    { label: 'Owners', items: on.filter((m) => m.affiliation === 'owner') },
    { label: 'Admins', items: on.filter((m) => m.affiliation === 'admin') },
    { label: 'Online', items: on.filter((m) => !staff(m) && !visitor(m)) },
    // A visitor can read and cannot speak (a muted person, or a guest of a moderated room).
    { label: 'Visitors', items: on.filter((m) => !staff(m) && visitor(m)) },
    { label: 'Offline', items: members.filter((m) => !m.online) }
  ];
  return groups.filter((g) => g.items.length > 0);
}

const RANK: Record<Affiliation, number> = { owner: 3, admin: 2, member: 1, none: 0 };

/** I am a moderator of the room: by role, or by an affiliation that gives it. */
export function isModerator(mine: MemberItem | undefined): boolean {
  return (
    !!mine && (mine.role === 'Moderator' || mine.affiliation === 'owner' || mine.affiliation === 'admin')
  );
}

/** I may change the topic: a moderator always, anyone when the room config allows it. */
export function canSetTopic(mine: MemberItem | undefined, changeSubject: boolean): boolean {
  if (!mine || mine.role === 'Visitor') return false;
  return isModerator(mine) || changeSubject;
}

/**
 * I may kick or mute `target`. A moderator cannot act on an admin or an owner, unless my
 * affiliation is higher. Nobody acts on himself.
 */
export function canModerateMember(mine: MemberItem | undefined, target: MemberItem | undefined): boolean {
  if (!mine || !target || !isModerator(mine)) return false;
  if (mine.id === target.id) return false;
  const staff = target.affiliation === 'owner' || target.affiliation === 'admin';
  return !staff || RANK[mine.affiliation] > RANK[target.affiliation];
}
