import { describe, expect, it } from 'vitest';
import { actionLine, actionText } from './action';

describe('actionText', () => {
  it('reads the text after "/me "', () => {
    expect(actionText('/me waves')).toBe('waves');
    expect(actionText('/me   waves at you ')).toBe('waves at you');
  });

  it('needs the space and some text', () => {
    expect(actionText('/me')).toBeNull();
    expect(actionText('/me ')).toBeNull();
    expect(actionText('/mean thing')).toBeNull();
    expect(actionText('hello /me waves')).toBeNull();
    expect(actionText(' /me waves')).toBeNull();
  });

  it('is case sensitive, as the XEP is', () => {
    expect(actionText('/ME waves')).toBeNull();
  });
});

describe('actionLine', () => {
  it('shows an action with the name', () => {
    expect(actionLine('Alice', '/me waves')).toBe('* Alice waves');
  });

  it('leaves other bodies as they are', () => {
    expect(actionLine('Alice', 'hello')).toBe('hello');
    expect(actionLine('Alice', '/me')).toBe('/me');
  });
});
