// The logic of the data form renderer (DataFormView.svelte): what to show for a field,
// how to change its values, and what is wrong before the form goes out. Plain functions,
// so that Vitest can test them. The rules follow XEP-0004 and `Form::problems` in
// chord-core/src/forms.rs.
import type { CommandAction, CommandStep, DataForm, FormField, FormMedia } from '$lib/chord/types';

/** A field that the user sees. A hidden field carries data that the UI never shows. */
export function visibleFields(form: DataForm): FormField[] {
  return form.fields.filter((f) => f.kind !== 'hidden');
}

/** The name to show for a field: its label, or its name. */
export function fieldName(f: FormField): string {
  return f.label?.trim() || f.var || '';
}

/** The first value, or an empty string. */
export function firstValue(f: FormField): string {
  return f.values[0] ?? '';
}

/** A boolean field is on for "1" or "true" (XEP-0004, 3.3). */
export function isOn(f: FormField): boolean {
  const v = firstValue(f).trim();
  return v === '1' || v === 'true';
}

/** The value of a boolean as the form sends it. */
export function boolValue(on: boolean): string[] {
  return [on ? '1' : '0'];
}

/** The text of a text-multi field: one value for each line. */
export function toMultiline(values: string[]): string {
  return values.join('\n');
}

export function fromMultiline(text: string): string[] {
  return text === '' ? [] : text.split('\n');
}

/** Add or remove one value of a list-multi field. The order of the options stays. */
export function toggleValue(f: FormField, value: string, on: boolean): string[] {
  const has = new Set(f.values);
  if (on) has.add(value);
  else has.delete(value);
  const known = f.options.map((o) => o.value).filter((v) => has.has(v));
  // A value that is not an option (the server allows it) stays at the end.
  const extra = [...has].filter((v) => !f.options.some((o) => o.value === v));
  return [...known, ...extra];
}

/** The text that a list option shows. */
export function optionText(o: { label: string | null; value: string }): string {
  return o.label?.trim() || o.value;
}

/** A list-single field needs an option that is selected. With none, the first one shows. */
export function selectedOption(f: FormField): string {
  const v = firstValue(f);
  return f.options.some((o) => o.value === v) ? v : '';
}

// A JID in a form: [local@]domain[/resource]. The server does the strict check.
const JID = /^([^@/\s]+@)?[^@/\s]+(\/\S.*)?$/;

export function isJid(s: string): boolean {
  return JID.test(s.trim());
}

/**
 * What is wrong with the answers. Empty when the form can go out. A required field that
 * is empty, and a jid field with a value that is not an address.
 */
export function problems(form: DataForm): string[] {
  const out: string[] = [];
  for (const f of form.fields) {
    if (f.kind === 'fixed' || f.kind === 'hidden') continue;
    const name = fieldName(f);
    const filled = f.values.some((v) => v.trim() !== '');
    if (f.required && f.kind !== 'boolean' && !filled) {
      out.push(`${name} is required`);
      continue;
    }
    if (f.kind === 'jid-single' || f.kind === 'jid-multi') {
      for (const v of f.values) {
        if (v.trim() !== '' && !isJid(v)) out.push(`${name}: ${v} is not a valid address`);
      }
    }
  }
  return out;
}

/** The problem of one field, for the message under it. */
export function fieldProblem(f: FormField): string | null {
  const one: DataForm = { kind: 'form', title: null, instructions: null, fields: [f] };
  return problems(one)[0] ?? null;
}

const IMAGE_URI = /^data:image\/(png|jpeg|gif|webp);base64,[A-Za-z0-9+/=]+$/;

/**
 * The URI of an image that is safe to show. Only an inline image qualifies. A link to a
 * remote image would tell that server the address of the reader, so the form shows it as
 * text and never loads it.
 */
export function inlineImage(m: FormMedia): string | null {
  return IMAGE_URI.test(m.uri) ? m.uri : null;
}

/** The media of a field that are not inline images. */
export function otherMedia(f: FormField): FormMedia[] {
  return f.media.filter((m) => inlineImage(m) === null);
}

/**
 * The form to send back. Only the UI state changed, so this is a copy with the same
 * fields. A `cancel` form has none.
 */
export function submission(form: DataForm): DataForm {
  return { ...form, kind: 'submit', fields: form.fields.map((f) => ({ ...f, values: [...f.values] })) };
}

export type StepAction = 'next' | 'prev' | 'complete';

/** The buttons of a command step. `cancel` is always there, and not in this list. */
export function stepButtons(
  actions: readonly string[],
  defaultAction: string | null
): { action: StepAction; primary: boolean }[] {
  const order: StepAction[] = ['prev', 'next', 'complete'];
  let shown = order.filter((a) => actions.includes(a));
  // No actions means a single step: the only way on is `complete`.
  if (shown.length === 0) shown = ['complete'];
  const primary = shown.find((a) => a === defaultAction) ?? shown[shown.length - 1];
  return shown.map((action) => ({ action, primary: action === primary }));
}

// ---- ad-hoc command steps ----

/** What the UI sends for one step: the pieces of `commandStep`. */
export interface StepRequest {
  sessionId: string | null;
  action: CommandAction;
  form: DataForm | null;
}

/**
 * The request for the next call, or the problems that stop it. `prev` and `cancel` send
 * no form and need no valid answers: the user goes back or leaves.
 */
export function stepRequest(
  step: Pick<CommandStep, 'sessionId' | 'form'>,
  action: CommandAction,
  form: DataForm | null
): { request: StepRequest } | { problems: string[] } {
  if (action === 'prev' || action === 'cancel') {
    return { request: { sessionId: step.sessionId, action, form: null } };
  }
  if (form) {
    const found = problems(form);
    if (found.length > 0) return { problems: found };
  }
  return { request: { sessionId: step.sessionId, action, form: form ? submission(form) : null } };
}

/** True when the command is over: the UI shows the result and a "Done" button. */
export function isFinished(step: Pick<CommandStep, 'status'>): boolean {
  return step.status !== 'executing';
}

/** The name of a command in a list: its name, or its node. */
export function commandTitle(item: { name: string | null; node: string }): string {
  return item.name?.trim() || item.node;
}

// ---- registration ----

/** A legacy registration field (XEP-0077, 3.1) as the UI shows it. */
export interface LegacyField {
  name: string;
  label: string;
  secret: boolean;
}

const LEGACY_LABELS: Record<string, string> = {
  username: 'Username',
  password: 'Password',
  nick: 'Nickname',
  name: 'Full name',
  first: 'First name',
  last: 'Last name',
  email: 'Email address',
  address: 'Address',
  city: 'City',
  state: 'State',
  zip: 'Postal code',
  phone: 'Phone number',
  url: 'Web page',
  date: 'Date',
  misc: 'Other',
  text: 'Note',
  key: 'Key'
};

export function legacyFields(names: readonly string[]): LegacyField[] {
  return names.map((name) => ({
    name,
    label: LEGACY_LABELS[name] ?? name,
    secret: name === 'password'
  }));
}

/** The answer of the legacy fields, in the order that the server named them. */
export function legacyAnswer(
  names: readonly string[],
  values: Record<string, string>
): [string, string][] {
  return names.map((n) => [n, (values[n] ?? '').trim()]);
}

/** The names of the legacy fields that have no value. All of them are required. */
export function legacyMissing(names: readonly string[], values: Record<string, string>): string[] {
  return legacyFields(names)
    .filter((f) => (values[f.name] ?? '').trim() === '')
    .map((f) => f.label);
}

/** The username and the password that a registration form asks for, if it has them. */
export function credentialsOf(form: DataForm): { username: string; password: string } {
  const get = (v: string) => form.fields.find((f) => f.var === v)?.values[0]?.trim() ?? '';
  return { username: get('username'), password: form.fields.find((f) => f.var === 'password')?.values[0] ?? '' };
}

/** The server part of an address, or the text itself when it has no `@`. */
export function domainOf(input: string): string {
  const s = input.trim();
  const at = s.lastIndexOf('@');
  return (at >= 0 ? s.slice(at + 1) : s).toLowerCase();
}

/**
 * The text for a registration error. A server that keeps registration closed answers
 * with `forbidden`, `not-allowed` or `service-unavailable`. Most servers do this, so the
 * text tells the user what to do next.
 */
export function registerErrorText(error: unknown): string {
  const e = error as { code?: string; message?: string } | null;
  const message = typeof e?.message === 'string' ? e.message : String(error);
  if (e?.code === 'server' && /Forbidden|NotAllowed|ServiceUnavailable|FeatureNotImplemented/.test(message)) {
    return 'This server does not let people create an account in the app. Ask its owner for an invite link, or use its website.';
  }
  if (e?.code === 'server' && /Conflict/.test(message)) {
    return 'That username is taken. Try another one.';
  }
  if (e?.code === 'server' && /NotAcceptable/.test(message)) {
    return 'The server did not accept these answers. Check each field, and the password rules of the server.';
  }
  if (e?.code === 'unreachable') return "Can't reach the server. Check the address and your connection.";
  if (e?.code === 'tlsInvalid') return "The server's certificate is not valid, so Chord did not connect.";
  if (e?.code === 'timeout') return 'The server did not answer in time. Try again.';
  return message;
}
