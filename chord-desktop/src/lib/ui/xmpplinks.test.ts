import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { session } from './session.svelte';
import { ui } from './ui.svelte';
import { xmppLinks } from './xmpplinks.svelte';

const SPACE = 'xmpp:chat.foid.space?pubsub;action=subscribe;node=synth';
const ROOM = 'xmpp:hangar@chat.foid.space?join';

beforeEach(() => {
  vi.useFakeTimers();
  xmppLinks.asking = null;
  session.force('signed-out');
});
afterEach(() => {
  vi.useRealTimers();
});

describe('opening a link', () => {
  it('keeps a link until the session is up', () => {
    xmppLinks.open(SPACE, true);
    expect(xmppLinks.asking).toBeNull();
    session.force('restoring');
    xmppLinks.flush();
    expect(xmppLinks.asking).toBeNull();
    session.force('connected');
    xmppLinks.flush();
    expect(xmppLinks.asking).toMatchObject({ kind: 'space', node: 'synth' });
  });

  it('asks at once when the session is up, and never acts by itself', () => {
    session.force('connected');
    xmppLinks.open(ROOM);
    expect(xmppLinks.asking).toMatchObject({ kind: 'room', jid: 'hangar@chat.foid.space' });
  });

  it('shows the queued links one after the other', () => {
    xmppLinks.open('xmpp:sam@chord.example');
    xmppLinks.open(ROOM);
    session.force('connected');
    xmppLinks.flush();
    expect(xmppLinks.asking).toMatchObject({ kind: 'chat' });
    xmppLinks.dismiss();
    expect(xmppLinks.asking).toMatchObject({ kind: 'room' });
    xmppLinks.dismiss();
    expect(xmppLinks.asking).toBeNull();
  });

  it('tells the user about a link that is not valid', () => {
    session.force('connected');
    xmppLinks.open('xmpp:sam@chord.example?remove');
    expect(xmppLinks.asking).toBeNull();
    expect(ui.toast).toBe('This link is not valid.');
  });

  it('counts a repeat from the OS once', () => {
    session.force('connected');
    xmppLinks.open('xmpp:kim@chord.example', true);
    xmppLinks.dismiss();
    xmppLinks.open('xmpp:kim@chord.example', true);
    expect(xmppLinks.asking).toBeNull();
    vi.advanceTimersByTime(2000);
    xmppLinks.open('xmpp:kim@chord.example', true);
    expect(xmppLinks.asking).toMatchObject({ kind: 'chat' });
  });
});

describe('the sample answers', () => {
  // The tests above started other links. Each link asks once, so this test uses new ones.
  it('knows a public space and a room, and refuses the rest', async () => {
    const space = { kind: 'space', service: 'chat.foid.space', node: 'sourdough' } as const;
    const room = { kind: 'room', jid: 'launch-ops-safety@chat.foid.space', password: null } as const;
    const gone = { kind: 'space', service: 'chat.foid.space', node: 'nothing' } as const;
    for (const l of [space, room, gone]) xmppLinks.request(l);
    expect(xmppLinks.get(space)).toBeUndefined();
    await vi.advanceTimersByTimeAsync(300);
    expect(xmppLinks.get(space)).toMatchObject({ kind: 'space', name: 'Sourdough' });
    expect(xmppLinks.get(room)).toMatchObject({ kind: 'room', name: 'safety' });
    expect(xmppLinks.get(gone)).toBeNull();
  });
});
