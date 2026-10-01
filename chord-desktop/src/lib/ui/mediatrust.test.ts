import { describe, expect, it } from 'vitest';
import { hostOf, isLocalHost, mayAutoLoad, webUrl } from './mediatrust';

const contacts = new Set(['ann@chat.example']);
const isContact = (a: string) => contacts.has(a);

describe('mayAutoLoad', () => {
  it('loads for me and for contacts', () => {
    expect(mayAutoLoad('me@chat.example', 'me@chat.example', isContact, false)).toBe(true);
    expect(mayAutoLoad('ann@chat.example/phone', 'me@chat.example', isContact, false)).toBe(true);
  });
  it('waits for a click for a stranger', () => {
    expect(mayAutoLoad('bob@other.example', 'me@chat.example', isContact, false)).toBe(false);
  });
  it('treats a room nick as a stranger', () => {
    expect(mayAutoLoad('room@muc.example/ann', 'me@chat.example', isContact, false)).toBe(false);
  });
  it('loads for everyone when the user allows it', () => {
    expect(mayAutoLoad('room@muc.example/bob', 'me@chat.example', isContact, true)).toBe(true);
  });
});

describe('webUrl and hostOf', () => {
  it('takes http and https only', () => {
    expect(webUrl('https://a.example/x')).not.toBeNull();
    expect(webUrl('http://a.example/x')).not.toBeNull();
    for (const bad of ['javascript:alert(1)', 'file:///etc/passwd', 'data:text/html,x', 'x', ''])
      expect(webUrl(bad)).toBeNull();
  });
  it('shows the real host', () => {
    expect(hostOf('https://evil.example/login/photo.jpg')).toBe('evil.example');
    expect(hostOf('https://photo.jpg@evil.example/')).toBe('evil.example');
    expect(hostOf('nonsense')).toBe('');
  });
});

describe('isLocalHost', () => {
  it('refuses local and private hosts', () => {
    for (const u of [
      'http://localhost/a',
      'http://foo.localhost/a',
      'http://nas.local/a',
      'http://printer/a',
      'http://127.0.0.1:8080/a',
      'https://10.0.0.5/a',
      'https://192.168.1.10:9000/',
      'https://172.16.0.1/',
      'http://169.254.169.254/latest',
      'http://[::1]/a',
      'http://[fd00::1]/a',
      'http://[::ffff:7f00:1]/a',
      'http://2130706433/a',
      'http://0x7f.1/a',
      'ftp://x.example/a'
    ])
      expect(isLocalHost(u), u).toBe(true);
  });
  it('allows public hosts', () => {
    for (const u of [
      'https://up.example.org/a.png',
      'https://8.8.8.8/a',
      'https://172.32.0.1/a',
      'https://[2606:4700::1111]/a'
    ])
      expect(isLocalHost(u), u).toBe(false);
  });
});
