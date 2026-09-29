// Time and text helpers. Wording follows the style guide: short and lower case.

const pad = (n: number) => String(n).padStart(2, '0');

export function clock(ts: number): string {
  const d = new Date(ts);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function startOfDay(ts: number): number {
  return new Date(ts).setHours(0, 0, 0, 0);
}

export function dayDiff(ts: number, now = Date.now()): number {
  return Math.round((startOfDay(now) - startOfDay(ts)) / 86_400_000);
}

const shortDate = new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short' });
const longDate = new Intl.DateTimeFormat(undefined, {
  weekday: 'long',
  day: 'numeric',
  month: 'long',
  year: 'numeric'
});

/** Message time: "today 15:04", "yesterday 21:10", "28 Sep 15:04". */
export function stamp(ts: number): string {
  const diff = dayDiff(ts);
  if (diff === 0) return `today ${clock(ts)}`;
  if (diff === 1) return `yesterday ${clock(ts)}`;
  return `${shortDate.format(ts)} ${clock(ts)}`;
}

export function dayLabel(ts: number): string {
  return longDate.format(ts);
}

export function sameDay(a: number, b: number): boolean {
  return startOfDay(a) === startOfDay(b);
}

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function domainOf(address: string): string {
  return address.split('@')[1] ?? '';
}

/** One or two letters for an avatar or circle icon. */
export function initials(name: string): string {
  const parts = name.trim().split(/[\s-]+/).filter(Boolean);
  if (!parts.length) return '?';
  if (parts.length === 1) return parts[0][0].toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}

/** Avatar tint. Hues skip the purple range on purpose. */
const HUES = [12, 32, 52, 95, 150, 175, 195, 215, 345];
export function tint(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return `hsl(${HUES[h % HUES.length]} 42% 30%)`;
}

export function typingText(names: string[]): string {
  if (names.length === 0) return '';
  if (names.length === 1) return `${names[0]} is typing…`;
  if (names.length === 2) return `${names[0]} and ${names[1]} are typing…`;
  return 'Several people are typing…';
}
