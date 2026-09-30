import { describe, expect, it } from 'vitest';
import type { DataForm, FormField } from '$lib/chord/types';
import {
  boolValue,
  commandTitle,
  credentialsOf,
  domainOf,
  fieldName,
  fieldProblem,
  fromMultiline,
  inlineImage,
  isFinished,
  isJid,
  isOn,
  legacyAnswer,
  legacyFields,
  legacyMissing,
  optionText,
  otherMedia,
  problems,
  registerErrorText,
  selectedOption,
  stepButtons,
  stepRequest,
  submission,
  toMultiline,
  toggleValue,
  visibleFields
} from './forms';

function field(patch: Partial<FormField>): FormField {
  return {
    var: 'v',
    kind: 'text-single',
    label: null,
    desc: null,
    required: false,
    values: [],
    options: [],
    media: [],
    ...patch
  };
}

function form(...fields: FormField[]): DataForm {
  return { kind: 'form', title: null, instructions: null, fields };
}

describe('showing fields', () => {
  it('hides the hidden fields and keeps the fixed ones', () => {
    const f = form(
      field({ var: 'FORM_TYPE', kind: 'hidden' }),
      field({ var: null, kind: 'fixed', values: ['Section'] }),
      field({ var: 'a' })
    );
    expect(visibleFields(f).map((x) => x.var)).toEqual([null, 'a']);
  });

  it('names a field by its label, then its name', () => {
    expect(fieldName(field({ label: ' Room title ' }))).toBe('Room title');
    expect(fieldName(field({ label: null, var: 'muc#x' }))).toBe('muc#x');
    expect(fieldName(field({ label: '', var: null }))).toBe('');
  });

  it('reads a boolean as the XEP does', () => {
    expect(isOn(field({ kind: 'boolean', values: ['1'] }))).toBe(true);
    expect(isOn(field({ kind: 'boolean', values: ['true'] }))).toBe(true);
    expect(isOn(field({ kind: 'boolean', values: ['0'] }))).toBe(false);
    expect(isOn(field({ kind: 'boolean', values: ['false'] }))).toBe(false);
    expect(isOn(field({ kind: 'boolean', values: [] }))).toBe(false);
    expect(boolValue(true)).toEqual(['1']);
    expect(boolValue(false)).toEqual(['0']);
  });

  it('shows the option label, or the value', () => {
    expect(optionText({ label: 'Moderators', value: 'moderators' })).toBe('Moderators');
    expect(optionText({ label: null, value: 'anyone' })).toBe('anyone');
    expect(optionText({ label: '  ', value: 'none' })).toBe('none');
  });

  it('selects an option only when the value is one of them', () => {
    const f = field({
      kind: 'list-single',
      values: ['30'],
      options: [
        { label: null, value: '10' },
        { label: null, value: '30' }
      ]
    });
    expect(selectedOption(f)).toBe('30');
    expect(selectedOption({ ...f, values: ['99'] })).toBe('');
    expect(selectedOption({ ...f, values: [] })).toBe('');
  });
});

describe('changing values', () => {
  const tags = field({
    kind: 'list-multi',
    values: ['b'],
    options: [
      { label: null, value: 'a' },
      { label: null, value: 'b' },
      { label: null, value: 'c' }
    ]
  });

  it('adds and removes a value and keeps the option order', () => {
    expect(toggleValue(tags, 'a', true)).toEqual(['a', 'b']);
    expect(toggleValue(tags, 'c', true)).toEqual(['b', 'c']);
    expect(toggleValue(tags, 'b', false)).toEqual([]);
    expect(toggleValue({ ...tags, values: ['c', 'a'] }, 'b', true)).toEqual(['a', 'b', 'c']);
  });

  it('keeps a value that is not an option', () => {
    expect(toggleValue({ ...tags, values: ['x'] }, 'a', true)).toEqual(['a', 'x']);
    expect(toggleValue({ ...tags, values: ['x'] }, 'a', false)).toEqual(['x']);
  });

  it('turns lines into values and back', () => {
    expect(fromMultiline('one\ntwo')).toEqual(['one', 'two']);
    expect(fromMultiline('')).toEqual([]);
    expect(fromMultiline('a\n\nb')).toEqual(['a', '', 'b']);
    expect(toMultiline(['one', 'two'])).toBe('one\ntwo');
    expect(toMultiline([])).toBe('');
  });
});

describe('checking a form', () => {
  it('needs a value for a required field', () => {
    const f = form(field({ label: 'Name', required: true, values: ['  '] }));
    expect(problems(f)).toEqual(['Name is required']);
    f.fields[0].values = ['Lobby'];
    expect(problems(f)).toEqual([]);
  });

  it('does not ask a required boolean for a value, or a hidden or fixed field', () => {
    const f = form(
      field({ kind: 'boolean', required: true }),
      field({ kind: 'hidden', required: true }),
      field({ var: null, kind: 'fixed', required: true })
    );
    expect(problems(f)).toEqual([]);
  });

  it('checks the JIDs of a jid field', () => {
    const f = form(
      field({ label: 'Owners', kind: 'jid-multi', values: ['a@example.org', 'b@example.org/phone', ''] })
    );
    expect(problems(f)).toEqual([]);
    f.fields[0].values.push('not a jid@@');
    expect(problems(f)).toEqual(['Owners: not a jid@@ is not a valid address']);
    expect(isJid('example.org')).toBe(true);
    expect(isJid('a@b@c')).toBe(false);
    expect(isJid('a b@example.org')).toBe(false);
  });

  it('gives the problem of one field', () => {
    expect(fieldProblem(field({ label: 'Name', required: true }))).toBe('Name is required');
    expect(fieldProblem(field({ required: true, values: ['x'] }))).toBeNull();
  });
});

describe('media', () => {
  const media = (uri: string) => ({ uri, mime: null, width: null, height: null });

  it('shows only an inline image', () => {
    expect(inlineImage(media('data:image/png;base64,iVBORw0KGgo='))).not.toBeNull();
    expect(inlineImage(media('data:image/jpeg;base64,/9j/4AAQ'))).not.toBeNull();
  });

  it('never loads a remote image or a script', () => {
    expect(inlineImage(media('https://example.org/captcha.png'))).toBeNull();
    expect(inlineImage(media('http://example.org/captcha.png'))).toBeNull();
    expect(inlineImage(media('cid:sha1+a@bob.xmpp.org'))).toBeNull();
    expect(inlineImage(media('data:image/svg+xml;base64,PHN2Zz4='))).toBeNull();
    expect(inlineImage(media('data:text/html;base64,PHNjcmlwdD4='))).toBeNull();
    expect(inlineImage(media('data:image/png;base64,AA" onerror="x'))).toBeNull();
  });

  it('lists the media that the form cannot show', () => {
    const f = field({
      media: [media('data:image/png;base64,AA=='), media('https://example.org/a.ogg')]
    });
    expect(otherMedia(f).map((m) => m.uri)).toEqual(['https://example.org/a.ogg']);
  });
});

describe('the answer', () => {
  it('is a submit form with copies of the values', () => {
    const f = form(field({ var: 'a', values: ['1'] }));
    const out = submission(f);
    expect(out.kind).toBe('submit');
    out.fields[0].values.push('2');
    expect(f.fields[0].values).toEqual(['1']);
    expect(out.fields[0].var).toBe('a');
  });
});

describe('the buttons of a command step', () => {
  it('shows the actions in a fixed order and marks the default', () => {
    expect(stepButtons(['next', 'prev'], 'next')).toEqual([
      { action: 'prev', primary: false },
      { action: 'next', primary: true }
    ]);
    expect(stepButtons(['prev', 'complete'], 'complete')).toEqual([
      { action: 'prev', primary: false },
      { action: 'complete', primary: true }
    ]);
  });

  it('makes the last action the default when the service names none', () => {
    expect(stepButtons(['next', 'complete'], null).map((b) => b.primary)).toEqual([false, true]);
  });

  it('offers only complete for a one-step command', () => {
    expect(stepButtons([], null)).toEqual([{ action: 'complete', primary: true }]);
  });

  it('ignores the actions that are not buttons', () => {
    expect(stepButtons(['execute', 'cancel'], 'execute')).toEqual([{ action: 'complete', primary: true }]);
  });
});

describe('running a command', () => {
  const step = { sessionId: 's9', form: null };

  it('sends the answers with the session for next and complete', () => {
    const f = form(field({ var: 'name', values: ['Lobby'] }));
    const r = stepRequest(step, 'next', f);
    expect(r).toEqual({
      request: { sessionId: 's9', action: 'next', form: { ...f, kind: 'submit' } }
    });
  });

  it('stops a next with a missing required value', () => {
    const f = form(field({ label: 'Name', required: true }));
    expect(stepRequest(step, 'complete', f)).toEqual({ problems: ['Name is required'] });
  });

  it('lets the user go back or leave with an incomplete form', () => {
    const f = form(field({ label: 'Name', required: true }));
    expect(stepRequest(step, 'prev', f)).toEqual({
      request: { sessionId: 's9', action: 'prev', form: null }
    });
    expect(stepRequest(step, 'cancel', f)).toEqual({
      request: { sessionId: 's9', action: 'cancel', form: null }
    });
  });

  it('sends no form for a step that has none', () => {
    expect(stepRequest(step, 'complete', null)).toEqual({
      request: { sessionId: 's9', action: 'complete', form: null }
    });
  });

  it('knows when the command is over', () => {
    expect(isFinished({ status: 'executing' })).toBe(false);
    expect(isFinished({ status: 'completed' })).toBe(true);
    expect(isFinished({ status: 'canceled' })).toBe(true);
  });

  it('names a command by its name or its node', () => {
    expect(commandTitle({ name: 'Ping', node: 'ping' })).toBe('Ping');
    expect(commandTitle({ name: null, node: 'urn:xmpp:invite#invite' })).toBe('urn:xmpp:invite#invite');
  });
});

describe('registration', () => {
  it('labels the legacy fields and hides the password', () => {
    const fields = legacyFields(['username', 'password', 'email', 'x-custom']);
    expect(fields.map((f) => f.label)).toEqual(['Username', 'Password', 'Email address', 'x-custom']);
    expect(fields.map((f) => f.secret)).toEqual([false, true, false, false]);
  });

  it('answers in the order of the server, with trimmed values', () => {
    expect(legacyAnswer(['username', 'password'], { password: 'p w', username: ' bob ' })).toEqual([
      ['username', 'bob'],
      ['password', 'p w']
    ]);
  });

  it('lists the legacy fields that have no value', () => {
    expect(legacyMissing(['username', 'password', 'email'], { username: 'a', email: ' ' })).toEqual([
      'Password',
      'Email address'
    ]);
  });

  it('reads the credentials from a form', () => {
    const f = form(
      field({ var: 'username', values: [' bob '] }),
      field({ var: 'password', values: ['secret'] })
    );
    expect(credentialsOf(f)).toEqual({ username: 'bob', password: 'secret' });
    expect(credentialsOf(form())).toEqual({ username: '', password: '' });
  });

  it('takes the server from an address', () => {
    expect(domainOf('Bob@Chat.Example.org')).toBe('chat.example.org');
    expect(domainOf(' chat.example.org ')).toBe('chat.example.org');
    expect(domainOf('')).toBe('');
  });

  it('explains a closed registration', () => {
    const closed = { code: 'server', message: 'the server refused: Forbidden: Access denied by service policy' };
    expect(registerErrorText(closed)).toMatch(/does not let people create an account/);
    expect(registerErrorText({ code: 'server', message: 'the server refused: NotAllowed' })).toMatch(
      /invite link/
    );
    expect(registerErrorText({ code: 'server', message: 'the server refused: Conflict' })).toMatch(/taken/);
    expect(registerErrorText({ code: 'server', message: 'the server refused: NotAcceptable' })).toMatch(
      /did not accept/
    );
    expect(registerErrorText({ code: 'unreachable', message: 'x' })).toMatch(/reach the server/);
    expect(registerErrorText({ code: 'timeout', message: 'x' })).toMatch(/in time/);
    expect(registerErrorText({ code: 'invalid', message: 'bad thing' })).toBe('bad thing');
  });
});
