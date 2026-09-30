import { describe, expect, it } from 'vitest';
import type { DataForm } from '$lib/chord/types';
import {
  captchaProblems,
  createRoomQuestion,
  roomIsMissing,
  sharePasswordQuestion
} from './roomjoin';

describe('the question before a room is made', () => {
  it('reads an item-not-found error as a room that does not exist', () => {
    expect(roomIsMissing({ code: 'itemNotFound', message: 'x' })).toBe(true);
  });

  it('lets any other error, or no error object, join as before', () => {
    expect(roomIsMissing({ code: 'server', message: 'x' })).toBe(false);
    expect(roomIsMissing({ code: 'timeout', message: 'x' })).toBe(false);
    expect(roomIsMissing(null)).toBe(false);
    expect(roomIsMissing(undefined)).toBe(false);
    expect(roomIsMissing('itemNotFound')).toBe(false);
  });

  it('names the address and the room that would be made', () => {
    const text = createRoomQuestion('devs@rooms.example.org');
    expect(text).toContain('devs@rooms.example.org');
    expect(text).toContain('named devs');
    expect(text).toContain('typo');
  });
});

describe('the password in the bookmark', () => {
  it('says what the server keeps and that a no keeps it on this device', () => {
    const text = sharePasswordQuestion('devs@rooms.example.org');
    expect(text).toContain('devs');
    expect(text).toContain('plain text');
    expect(text).toContain('this device only');
  });
});

describe('the CAPTCHA of a room', () => {
  const form = (answer: string): DataForm => ({
    kind: 'form',
    title: null,
    instructions: null,
    fields: [
      {
        var: 'challenge',
        kind: 'hidden',
        label: null,
        desc: null,
        required: false,
        values: ['c-1'],
        options: [],
        media: []
      },
      {
        var: 'ocr',
        kind: 'text-single',
        label: 'Type the letters',
        desc: null,
        required: true,
        values: answer ? [answer] : [],
        options: [],
        media: []
      }
    ]
  });

  it('needs an answer before it goes out', () => {
    expect(captchaProblems(form(''))).toEqual(['Type the letters is required']);
    expect(captchaProblems(form('xk3p'))).toEqual([]);
  });
});
