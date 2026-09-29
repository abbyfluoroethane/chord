import { expect, it } from 'vitest';
import { channelLink } from './messagelink';

it('builds XEP-0147 links', () => {
  expect(channelLink('ops@chat.example', 'channel')).toBe('xmpp:ops@chat.example?join');
  expect(channelLink('rin@example.org', 'dm')).toBe('xmpp:rin@example.org?message');
});
