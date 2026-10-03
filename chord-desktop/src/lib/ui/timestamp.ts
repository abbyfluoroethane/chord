// Discord timestamps: <t:1735689600:R>. This file only formats. It holds no state.

export type TimeStyle = 't' | 'T' | 'd' | 'D' | 'f' | 'F' | 'R';

export interface TimeOptions {
  locale?: string;
  timeZone?: string;
  /** 12-hour, 24-hour, or none for the locale. */
  hourCycle?: 'h12' | 'h23';
}

/** The Date for a count of seconds, or null when the number is not a valid date. */
export function dateOf(seconds: number): Date | null {
  if (!Number.isFinite(seconds)) return null;
  const d = new Date(seconds * 1000);
  return Number.isNaN(d.getTime()) ? null : d;
}

const SHORT_TIME = { hour: 'numeric', minute: '2-digit' } as const;
const LONG_TIME = { hour: 'numeric', minute: '2-digit', second: '2-digit' } as const;
const SHORT_DATE = { year: 'numeric', month: '2-digit', day: '2-digit' } as const;
const LONG_DATE = { year: 'numeric', month: 'long', day: 'numeric' } as const;
const LONG_DAY = { weekday: 'long', ...LONG_DATE } as const;

/** The relative text: "in 5 minutes" or "3 days ago". */
export function relative(date: Date, now: number, locale?: string): string {
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const s = Math.round((date.getTime() - now) / 1000);
  const a = Math.abs(s);
  if (a < 60) return rtf.format(s, 'second');
  if (a < 3600) return rtf.format(Math.round(s / 60), 'minute');
  if (a < 86400) return rtf.format(Math.round(s / 3600), 'hour');
  if (a < 86400 * 30) return rtf.format(Math.round(s / 86400), 'day');
  if (a < 86400 * 365) return rtf.format(Math.round(s / (86400 * 30)), 'month');
  return rtf.format(Math.round(s / (86400 * 365)), 'year');
}

/** The text of a timestamp in one style. `now` matters only for the R style. */
export function formatTimestamp(
  date: Date,
  style: TimeStyle,
  now: number,
  opt: TimeOptions = {}
): string {
  const { locale, timeZone, hourCycle } = opt;
  const f = (o: Intl.DateTimeFormatOptions) =>
    new Intl.DateTimeFormat(locale, { ...o, timeZone, hourCycle }).format(date);
  switch (style) {
    case 't':
      return f(SHORT_TIME);
    case 'T':
      return f(LONG_TIME);
    case 'd':
      return f(SHORT_DATE);
    case 'D':
      return f(LONG_DATE);
    case 'F':
      return `${f(LONG_DAY)} ${f(SHORT_TIME)}`;
    case 'R':
      return relative(date, now, locale);
    default:
      return `${f(LONG_DATE)} ${f(SHORT_TIME)}`;
  }
}

/** The full date and time, for the hover title. */
export function fullTimestamp(date: Date, opt: TimeOptions = {}): string {
  return formatTimestamp(date, 'F', 0, opt);
}
