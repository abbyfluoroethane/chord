import { describe, expect, it } from 'vitest';
import { app } from './app.svelte';

describe('raw lists of the app state', () => {
  it('patchChannel makes a new row and leaves the other rows alone', () => {
    const before = app.channels.slice();
    const target = before.find((c) => c.unread > 0) ?? before[0];
    const others = before.filter((c) => c.jid !== target.jid);
    app.patchChannel(target.jid, { unread: target.unread + 5 });
    const row = app.channels.find((c) => c.jid === target.jid)!;
    expect(row).not.toBe(target);
    expect(row.unread).toBe(target.unread + 5);
    for (const o of others) expect(app.channels.find((c) => c.jid === o.jid)).toBe(o);
  });

  it('patchChannel does nothing when the values are equal', () => {
    const list = app.channels;
    const c = list[0];
    app.patchChannel(c.jid, { unread: c.unread });
    expect(app.channels).toBe(list);
  });

  it('a reaction makes a new row and leaves the other rows alone', () => {
    const jid = app.selectedJid;
    const list = app.timelines[jid];
    const m = list.find((x) => !x.retracted)!;
    app.toggleReaction(m.id, '🎉');
    const now = app.timelines[jid];
    const after = now.find((x) => x.id === m.id)!;
    expect(after).not.toBe(m);
    expect(after.reactions.some((r) => r.emoji === '🎉' && r.mine)).toBe(true);
    expect(m.reactions.some((r) => r.emoji === '🎉')).toBe(false);
    for (const x of list) if (x.id !== m.id) expect(now.find((y) => y.id === x.id)).toBe(x);
    app.toggleReaction(m.id, '🎉');
    expect(app.timelines[jid].find((x) => x.id === m.id)!.reactions.some((r) => r.emoji === '🎉')).toBe(false);
  });

  it('setMembers keeps the object of a member who did not change', () => {
    app.setMembers('test-space', [
      { id: 'a@x', name: 'A', role: null, affiliation: 'none', show: null, status: null, online: true, avatar: null },
      { id: 'b@x', name: 'B', role: null, affiliation: 'none', show: null, status: null, online: true, avatar: null }
    ]);
    const [a, b] = app.members['test-space'];
    app.setMembers('test-space', [{ ...a }, { ...b, online: false }]);
    const [a2, b2] = app.members['test-space'];
    expect(a2).toBe(a);
    expect(b2).not.toBe(b);
    expect(b2.online).toBe(false);
  });

  it('addMentionId marks an id once', () => {
    app.addMentionId('m:1');
    const ids = app.mentionIds;
    app.addMentionId('m:1');
    expect(app.mentionIds).toBe(ids);
    expect(app.mentionIds['m:1']).toBe(true);
  });
});
