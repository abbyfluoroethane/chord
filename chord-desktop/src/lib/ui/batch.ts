/** The notice for a batch action in which some steps failed, or null if none did. */
export function failureNote(failed: number, total: number, what: string): string | null {
  if (failed <= 0) return null;
  return `${failed} of ${total} ${what}${total === 1 ? '' : 's'} failed.`;
}

/**
 * Run `step` for each item and count the failures. A failure does not stop the rest.
 * The first error is kept, for a log line.
 */
export async function runBatch<T>(
  items: T[],
  step: (item: T) => Promise<unknown>
): Promise<{ failed: number; first: unknown }> {
  let failed = 0;
  let first: unknown;
  for (const item of items) {
    try {
      await step(item);
    } catch (e) {
      if (failed === 0) first = e;
      failed += 1;
    }
  }
  return { failed, first };
}
