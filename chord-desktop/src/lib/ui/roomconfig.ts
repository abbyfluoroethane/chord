// Read the current room settings out of the owner form (XEP-0045, 10.2), so that the
// channel settings can show them. Plain functions, so that Vitest can test them.
import type { DataForm } from '$lib/chord/types';
import { isOn } from './forms';

/** A yes/no choice. `keep` means the current value is not known. */
export type Flag = 'keep' | 'yes' | 'no';

/** The current value of a boolean field of the form, or `keep` when the form lacks it. */
export function flagOf(form: DataForm | null, name: string): Flag {
  const field = form?.fields.find((f) => f.var === name && f.kind === 'boolean');
  if (!field) return 'keep';
  return isOn(field) ? 'yes' : 'no';
}

/** What to send for a choice: nothing when it still has the value that the form showed. */
export function flagChange(initial: Flag, chosen: Flag): boolean | null {
  if (chosen === 'keep' || chosen === initial) return null;
  return chosen === 'yes';
}

/** The current text of a field, for the name of the room. Empty when the form lacks it. */
export function textOf(form: DataForm | null, name: string): string {
  return form?.fields.find((f) => f.var === name)?.values[0] ?? '';
}
