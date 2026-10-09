/** The appointment book's day (ORD-009 lite): 15-minute rows from 08:00 to 20:00, Algiers time. */

/** Algeria is UTC+1 all year (pos-core `time::ALGIERS_OFFSET_MS`, `src/lib/dates.ts`). */
const ALGIERS_OFFSET_MS = 3_600_000;
const DAY_MS = 86_400_000;

export const SLOT_MINUTES = 15;
export const OPENS = 8 * 60;
export const CLOSES = 20 * 60;
/** 48 rows of 15 minutes. */
export const SLOTS = (CLOSES - OPENS) / SLOT_MINUTES;

/** The instant `minutes` after midnight of `day` (`YYYY-MM-DD`) in Algiers. */
export function atMinutes(day: string, minutes: number): number {
  const [y = 0, m = 1, d = 1] = day.split("-").map(Number);
  return Date.UTC(y, m - 1, d) + minutes * 60_000 - ALGIERS_OFFSET_MS;
}

/** The minutes after midnight, in Algiers, of an instant. */
export function minutesOf(ms: number): number {
  const local = (((ms + ALGIERS_OFFSET_MS) % DAY_MS) + DAY_MS) % DAY_MS;
  return Math.floor(local / 60_000);
}

/** 600 → "10:00". */
export function hhmm(minutes: number): string {
  return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

/** "10:00" (a time field) → 600, or null. */
export function parseHhmm(value: string): number | null {
  const match = /^(\d{2}):(\d{2})$/.exec(value);
  if (!match) return null;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  return hours < 24 && minutes < 60 ? hours * 60 + minutes : null;
}

/** The row (0-based) a minute of the day falls in, kept inside the book. */
export function slotOf(minutes: number): number {
  return Math.min(SLOTS - 1, Math.max(0, Math.floor((minutes - OPENS) / SLOT_MINUTES)));
}

/** The day before or after `day`. */
export function shiftDay(day: string, days: number): string {
  return new Date(atMinutes(day, 12 * 60) + ALGIERS_OFFSET_MS + days * DAY_MS).toISOString().slice(0, 10);
}
