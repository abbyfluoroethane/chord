import { describe, expect, it } from 'vitest';
import type { DataForm, FormField } from '$lib/chord/types';
import { flagChange, flagOf, textOf } from './roomconfig';

function field(v: string, kind: FormField['kind'], values: string[]): FormField {
  return { var: v, kind, label: null, desc: null, required: false, values, options: [], media: [] };
}

const form: DataForm = {
  kind: 'form',
  title: null,
  instructions: null,
  fields: [
    field('muc#roomconfig_publicroom', 'boolean', ['1']),
    field('muc#roomconfig_membersonly', 'boolean', ['false']),
    field('muc#roomconfig_roomname', 'text-single', ['Town hall'])
  ]
};

describe('room config values', () => {
  it('reads a boolean field', () => {
    expect(flagOf(form, 'muc#roomconfig_publicroom')).toBe('yes');
    expect(flagOf(form, 'muc#roomconfig_membersonly')).toBe('no');
  });

  it('keeps the value when the form has no such field, or no form', () => {
    expect(flagOf(form, 'muc#roomconfig_other')).toBe('keep');
    expect(flagOf(null, 'muc#roomconfig_publicroom')).toBe('keep');
  });

  it('sends nothing for a choice that did not change', () => {
    expect(flagChange('yes', 'yes')).toBeNull();
    expect(flagChange('yes', 'no')).toBe(false);
    expect(flagChange('keep', 'yes')).toBe(true);
    expect(flagChange('no', 'keep')).toBeNull();
  });

  it('reads a text field', () => {
    expect(textOf(form, 'muc#roomconfig_roomname')).toBe('Town hall');
    expect(textOf(null, 'x')).toBe('');
  });
});
