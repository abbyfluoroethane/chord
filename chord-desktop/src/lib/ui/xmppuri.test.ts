import { describe, expect, it } from 'vitest';
import {
  looksLikeXmppUri,
  MAX_URI_LENGTH,
  parseAddress,
  parseRegisterLink,
  parseXmppUri,
  isUnsupportedLink,
  spaceInviteLink,
  xmppKey
} from './xmppuri';

const unknown = { kind: 'unknown' };

describe('space links', () => {
  it('reads the subscribe action', () => {
    expect(parseXmppUri('xmpp:pubsub.chord.example?pubsub;action=subscribe;node=launch-ops')).toEqual({
      kind: 'space',
      service: 'pubsub.chord.example',
      node: 'launch-ops'
    });
  });

  it('reads the XEP-0503 form with no action', () => {
    expect(parseXmppUri('xmpp:pubsub.chord.example?;node=launch-ops')).toEqual({
      kind: 'space',
      service: 'pubsub.chord.example',
      node: 'launch-ops'
    });
  });

  it('decodes the node', () => {
    expect(parseXmppUri('xmpp:pubsub.example.org?pubsub;action=subscribe;node=a%20b%3Bc%2Fd')).toMatchObject({
      node: 'a b;c/d'
    });
    expect(parseXmppUri('xmpp:pubsub.example.org?;node=caf%C3%A9')).toMatchObject({ node: 'café' });
  });

  it('writes the XEP-0503 form', () => {
    expect(spaceInviteLink('pubsub.chord.example', 'launch ops')).toBe(
      'xmpp:pubsub.chord.example?;node=launch%20ops'
    );
  });

  it('round-trips the link that the space dialog makes', () => {
    for (const node of ['plain', 'with space', 'semi;colon', 'a=b&c', 'ünï/cødé', '100%']) {
      const link = spaceInviteLink('pubsub.chord.example', node);
      expect(parseXmppUri(link)).toEqual({ kind: 'space', service: 'pubsub.chord.example', node });
    }
  });

  it('takes the first value of a repeated key', () => {
    expect(parseXmppUri('xmpp:p.example.org?;node=one;node=two')).toMatchObject({ node: 'one' });
  });

  it('accepts an account as the service', () => {
    expect(parseXmppUri('xmpp:Bob@Example.org?;node=x')).toMatchObject({ service: 'bob@example.org' });
  });

  it('rejects an empty or blank node', () => {
    expect(parseXmppUri('xmpp:p.example.org?pubsub;action=subscribe;node=')).toEqual(unknown);
    expect(parseXmppUri('xmpp:p.example.org?pubsub;action=subscribe;node=%20')).toEqual(unknown);
    expect(parseXmppUri('xmpp:p.example.org?;node=')).toEqual(unknown);
    expect(parseXmppUri('xmpp:p.example.org?pubsub;action=subscribe')).toEqual(unknown);
  });

  it('rejects an action other than subscribe', () => {
    expect(parseXmppUri('xmpp:p.example.org?pubsub;action=unsubscribe;node=x')).toEqual(unknown);
    expect(parseXmppUri('xmpp:p.example.org?pubsub;node=x')).toEqual(unknown);
    expect(parseXmppUri('xmpp:p.example.org?;action=delete;node=x')).toEqual(unknown);
  });

  it('rejects a node that is too long', () => {
    expect(parseXmppUri(`xmpp:p.example.org?;node=${'a'.repeat(1100)}`)).toEqual(unknown);
  });

  it('rejects a bad service', () => {
    expect(parseXmppUri('xmpp:?pubsub;action=subscribe;node=x')).toEqual(unknown);
    expect(parseXmppUri('xmpp:bad_host!?;node=x')).toEqual(unknown);
  });
});

describe('room links', () => {
  it('reads join', () => {
    expect(parseXmppUri('xmpp:dev@conference.example.org?join')).toEqual({
      kind: 'room',
      jid: 'dev@conference.example.org',
      password: null
    });
  });

  it('reads the password', () => {
    expect(parseXmppUri('xmpp:dev@conference.example.org?join;password=s%3Bcr%40t')).toEqual({
      kind: 'room',
      jid: 'dev@conference.example.org',
      password: 's;cr@t'
    });
  });

  it('is not case sensitive in the query type or the JID', () => {
    expect(parseXmppUri('XMPP:Dev@Conference.Example.ORG?JOIN')).toMatchObject({
      kind: 'room',
      jid: 'dev@conference.example.org'
    });
  });

  it('drops the resource', () => {
    expect(parseXmppUri('xmpp:dev@conference.example.org/nick?join')).toMatchObject({
      jid: 'dev@conference.example.org'
    });
  });

  it('needs a room address', () => {
    expect(parseXmppUri('xmpp:conference.example.org?join')).toEqual(unknown);
  });
});

describe('chat and contact links', () => {
  it('reads a plain address as a chat', () => {
    expect(parseXmppUri('xmpp:sam@chord.example')).toEqual({ kind: 'chat', jid: 'sam@chord.example', body: null });
  });

  it('reads message with a body', () => {
    expect(parseXmppUri('xmpp:sam@chord.example?message;body=Hello%20there%21')).toEqual({
      kind: 'chat',
      jid: 'sam@chord.example',
      body: 'Hello there!'
    });
    expect(parseXmppUri('xmpp:sam@chord.example?message')).toEqual({
      kind: 'chat',
      jid: 'sam@chord.example',
      body: null
    });
  });

  it('reads roster and subscribe as a contact', () => {
    expect(parseXmppUri('xmpp:sam@chord.example?roster;name=Sam%20R')).toEqual({
      kind: 'contact',
      jid: 'sam@chord.example',
      name: 'Sam R'
    });
    expect(parseXmppUri('xmpp:sam@chord.example?roster')).toMatchObject({ kind: 'contact', name: null });
    expect(parseXmppUri('xmpp:sam@chord.example?subscribe')).toEqual({
      kind: 'contact',
      jid: 'sam@chord.example',
      name: null
    });
  });

  it('reads the XEP-0379 preauth token of a roster or subscribe link', () => {
    expect(parseXmppUri('xmpp:sam@chord.example?roster;preauth=abc123')).toEqual({
      kind: 'contact',
      jid: 'sam@chord.example',
      name: null,
      preauth: 'abc123'
    });
    expect(parseXmppUri('xmpp:sam@chord.example?roster;name=Sam;preauth=a%3Db')).toMatchObject({
      name: 'Sam',
      preauth: 'a=b'
    });
    expect(parseXmppUri('xmpp:sam@chord.example?subscribe;preauth=t0k')).toMatchObject({ preauth: 't0k' });
    // An empty token is no token.
    expect(parseXmppUri('xmpp:sam@chord.example?roster;preauth=')).toEqual({
      kind: 'contact',
      jid: 'sam@chord.example',
      name: null
    });
  });

  it('reads a XEP-0401 sign-up link with parseRegisterLink only', () => {
    expect(parseRegisterLink('xmpp:Chord.Example?register;preauth=TOKEN1')).toEqual({
      domain: 'chord.example',
      preauth: 'TOKEN1'
    });
    expect(parseRegisterLink('xmpp:chord.example?register')).toEqual({ domain: 'chord.example', preauth: null });
    expect(parseRegisterLink('xmpp:sam@chord.example?register;preauth=x')).toBeNull();
    expect(parseRegisterLink('xmpp:chord.example?roster;preauth=x')).toBeNull();
    expect(parseRegisterLink('xmpp:bad_host!?register')).toBeNull();
    expect(parseRegisterLink('https://chord.example')).toBeNull();
    expect(parseXmppUri('xmpp:chord.example?register;preauth=TOKEN1')).toEqual(unknown);
  });

  it('decodes a percent-encoded address', () => {
    expect(parseXmppUri('xmpp:sam%40chord.example')).toMatchObject({ jid: 'sam@chord.example' });
  });

  it('ignores the fragment', () => {
    expect(parseXmppUri('xmpp:sam@chord.example#top')).toMatchObject({ jid: 'sam@chord.example' });
  });

  it('rejects an unknown action', () => {
    expect(parseXmppUri('xmpp:sam@chord.example?remove')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?unregister')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?command;node=x')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?')).toEqual(unknown);
  });

  it('keeps a long body that fits in the link', () => {
    expect(parseXmppUri(`xmpp:sam@chord.example?message;body=${'a'.repeat(1900)}`)).toMatchObject({ kind: 'chat' });
  });
});

describe('bad input', () => {
  it('rejects other schemes and tricks', () => {
    for (const s of [
      '',
      '   ',
      'javascript:alert(1)',
      'xmpp',
      'xmpp:',
      'http://example.org',
      'data:text/html,<b>x</b>',
      'javascript:alert(1)//xmpp:sam@chord.example',
      'file:///etc/passwd',
      'xmpps:sam@chord.example'
    ]) {
      expect(parseXmppUri(s)).toEqual(unknown);
    }
  });

  it('rejects a bad JID', () => {
    for (const s of [
      'xmpp:@chord.example',
      'xmpp:sam@',
      'xmpp:sam@@chord.example',
      'xmpp:sam',
      'xmpp:sa m@chord.example',
      'xmpp:sam@chord..example',
      'xmpp:sam@-chord.example',
      'xmpp:sam@chord!example.org',
      'xmpp:sam@chord.example/',
      'xmpp:s"m@chord.example',
      'xmpp:s<m@chord.example',
      "xmpp:s'm@chord.example",
      'xmpp:s%2Fm@chord.example',
      `xmpp:${'a'.repeat(1100)}@chord.example`
    ]) {
      expect(parseXmppUri(s)).toEqual(unknown);
    }
  });

  it('rejects bad percent escapes and control characters', () => {
    expect(parseXmppUri('xmpp:sam@chord.example?message;body=%E0%A4%A')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?message;body=%ZZ')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?message;body=%FF')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?message;body=a%00b')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam%00@chord.example')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example?join;password=a%0Ab')).toEqual(unknown);
    expect(parseXmppUri('xmpp:sam@chord.example\n?join')).toEqual(unknown);
  });

  it('rejects the authority form', () => {
    expect(parseXmppUri('xmpp://me@chord.example/sam@chord.example')).toEqual(unknown);
  });

  it('rejects a link that is too long', () => {
    expect(parseXmppUri(`xmpp:sam@chord.example?message;body=${'a'.repeat(MAX_URI_LENGTH)}`)).toEqual(unknown);
  });

  it('rejects whitespace inside the link', () => {
    expect(parseXmppUri('xmpp:sam@chord.example?message;body=a b')).toEqual(unknown);
  });

  it('never throws', () => {
    for (const s of ['xmpp:%', 'xmpp:?', 'xmpp:?;', 'xmpp:;;;', 'xmpp:a@b?join;=;=', 'xmpp:\ud800@x.org']) {
      expect(() => parseXmppUri(s)).not.toThrow();
    }
  });
});

describe('helpers', () => {
  it('looksLikeXmppUri checks the scheme only', () => {
    expect(looksLikeXmppUri('xmpp:a@b.org')).toBe(true);
    expect(looksLikeXmppUri('XMPP:x')).toBe(true);
    expect(looksLikeXmppUri('https://x.org')).toBe(false);
  });

  it('parseAddress takes a bare address', () => {
    expect(parseAddress(' Sam@Chord.Example/phone ')).toBe('sam@chord.example');
    expect(parseAddress('chord.example')).toBeNull();
    expect(parseAddress('')).toBeNull();
  });

  it('xmppKey names the target', () => {
    const space = parseXmppUri('xmpp:p.example.org?;node=x');
    const again = parseXmppUri('xmpp:P.example.org?pubsub;action=subscribe;node=x');
    if (space.kind === 'unknown' || again.kind === 'unknown') throw new Error('bad test');
    expect(xmppKey(space)).toBe(xmppKey(again));
    const room = parseXmppUri('xmpp:dev@c.example.org?join');
    if (room.kind === 'unknown') throw new Error('bad test');
    expect(xmppKey(room)).toBe('room:dev@c.example.org');
  });
});

describe('domains (IDNA)', () => {
  it('gives the ASCII form of a Unicode domain', () => {
    expect(parseXmppUri('xmpp:sam@Bücher.example?message')).toMatchObject({ jid: 'sam@xn--bcher-kva.example' });
    expect(parseXmppUri('xmpp:sam@xn--bcher-kva.example')).toMatchObject({ jid: 'sam@xn--bcher-kva.example' });
    expect(parseXmppUri('xmpp:chat.%D0%BF%D1%80%D0%B8%D0%BC%D0%B5%D1%80.example?;node=x')).toMatchObject({
      service: 'chat.xn--e1afmkfd.example'
    });
  });

  it('puts a full-width dot and upper case into the normal form', () => {
    expect(parseAddress('sam@Chord\u3002Example')).toBe('sam@chord.example');
  });

  it('accepts an underscore in a label', () => {
    expect(parseAddress('sam@_xmpp.test_host.example')).toBe('sam@_xmpp.test_host.example');
  });

  it('keeps the local part in NFC and lower case', () => {
    expect(parseAddress('Cafe\u0301@chord.example')).toBe('caf\u00e9@chord.example');
  });

  it('refuses names that URL would read as something else', () => {
    for (const d of ['a%41.example', 'a.example:8080', 'a b.example', 'a\\b.example', 'a#b.example', '-a.example', 'a-.example', 'a..example', '[::1]']) {
      expect(parseAddress(`sam@${d}`), d).toBeNull();
    }
    expect(parseAddress(`sam@${'a'.repeat(64)}.example`)).toBeNull();
  });
});

describe('keys are case sensitive', () => {
  it('reads the query type in any case but not a key', () => {
    expect(parseXmppUri('xmpp:p.example?PUBSUB;action=subscribe;node=x')).toMatchObject({ kind: 'space', node: 'x' });
    expect(parseXmppUri('xmpp:p.example?;Node=x')).toEqual(unknown);
    expect(parseXmppUri('xmpp:dev@c.example?join;Password=s')).toEqual({
      kind: 'room',
      jid: 'dev@c.example',
      password: null
    });
  });
});

describe('unsupported kinds of link', () => {
  it('names the known types that Chord does not act on', () => {
    for (const uri of [
      'xmpp:sam@chord.example?remove',
      'xmpp:sam@chord.example?vcard',
      'xmpp:sam@chord.example?invite;jid=a@b.example',
      'xmpp:sam@chord.example?command;node=x',
      'xmpp:p.example?pubsub;action=unsubscribe;node=x',
      'xmpp:p.example?;node=x;action=unsubscribe'
    ]) {
      expect(isUnsupportedLink(uri), uri).toBe(true);
      expect(parseXmppUri(uri), uri).toEqual(unknown);
    }
  });

  it('does not name a link that works, or one that is bad', () => {
    for (const uri of [
      'xmpp:sam@chord.example?message',
      'xmpp:p.example?;node=x',
      'xmpp:sam@chord.example',
      'xmpp:sam@chord.example?frobnicate',
      'xmpp:sam@chord.example?',
      'xmpp:?vcard',
      'xmpp://auth@chord.example/sam@chord.example?vcard',
      'http://example.org'
    ]) {
      expect(isUnsupportedLink(uri), uri).toBe(false);
    }
  });
});
