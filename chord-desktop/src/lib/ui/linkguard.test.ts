import { describe, expect, it } from 'vitest';
import { hostNote, hostWarning, linkHost, maskedMismatch, maskedTitle, toUnicodeHost } from './linkguard';

describe('toUnicodeHost', () => {
  it('decodes Punycode labels', () => {
    expect(toUnicodeHost('xn--bcher-kva.example')).toBe('bücher.example');
    expect(toUnicodeHost('xn--e1afmkfd.xn--p1ai')).toBe('пример.рф');
    expect(toUnicodeHost('plain.example')).toBe('plain.example');
  });

  it('keeps a label that does not decode', () => {
    expect(toUnicodeHost('xn--!!.example')).toBe('xn--!!.example');
  });
});

describe('hostNote', () => {
  it('is null for an ASCII host', () => {
    expect(hostNote('chat.example.org')).toBeNull();
    expect(hostWarning('chat.example.org')).toBeNull();
  });

  it('notes a host with other letters', () => {
    expect(hostNote('xn--bcher-kva.example')).toEqual({ unicode: 'bücher.example', mixed: false });
    expect(hostWarning('xn--bcher-kva.example')).toContain('bücher.example');
  });

  it('flags a Cyrillic letter inside a Latin name', () => {
    // "apple.example" with a Cyrillic "a" first
    const host = new URL('http://аpple.example/').hostname;
    expect(host.startsWith('xn--')).toBe(true);
    expect(hostNote(host)?.mixed).toBe(true);
    expect(hostWarning(host)).toMatch(/^Warning: .*mixes letters/);
  });

  it('does not flag a name in one script', () => {
    expect(hostNote('xn--e1afmkfd.xn--p1ai')?.mixed).toBe(false);
  });
});

describe('linkHost', () => {
  it('gives the ASCII host of a web link', () => {
    expect(linkHost('https://Example.org/a?b')).toBe('example.org');
    expect(linkHost('https://bücher.example/')).toBe('xn--bcher-kva.example');
    expect(linkHost('xmpp:a@b.example')).toBeNull();
    expect(linkHost('not a url')).toBeNull();
  });
});

describe('maskedMismatch', () => {
  it('flags a text that names another host', () => {
    expect(maskedMismatch('https://your-bank.com', 'https://evil.example')).toBe(true);
    expect(maskedMismatch('your-bank.com', 'https://evil.example/login')).toBe(true);
    expect(maskedMismatch('Go to www.paypal.com now', 'https://evil.example')).toBe(true);
    expect(maskedMismatch('https://example.org.evil.example', 'https://example.org')).toBe(true);
  });

  it('accepts a text with the same host, or no host', () => {
    expect(maskedMismatch('https://example.org/a', 'https://example.org/b')).toBe(false);
    expect(maskedMismatch('www.example.org', 'https://example.org')).toBe(false);
    expect(maskedMismatch('my blog', 'https://example.org')).toBe(false);
    expect(maskedMismatch('read this.', 'https://example.org')).toBe(false);
    expect(maskedMismatch('Example.ORG', 'https://example.org')).toBe(false);
  });

  it('ignores a target that is no web link', () => {
    expect(maskedMismatch('https://your-bank.com', 'xmpp:a@b.example')).toBe(false);
  });

  it('treats a look-alike host as another host', () => {
    expect(maskedMismatch('https://apple.com', 'https://аpple.com')).toBe(true);
  });
});

describe('maskedTitle', () => {
  it('puts the real host first', () => {
    expect(maskedTitle('https://Evil.example/x')).toBe('Opens evil.example\nhttps://Evil.example/x');
  });
});
