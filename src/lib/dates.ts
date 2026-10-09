/** Business days as `YYYY-MM-DD` (the backend's dates), with plain calendar arithmetic on UTC
 * dates: no time zone or daylight saving can shift a day. */

export const RANGE_PRESETS = [
  "today",
  "yesterday",
  "this_week",
  "last_week",
  "this_month",
  "last_month",
  "this_year",
  "custom",
] as const;
export type RangePreset = (typeof RANGE_PRESETS)[number];

/** Both ends included. */
export interface DateRange {
  from: string;
  to: string;
}

const DAY_MS = 86_400_000;
/** Algeria is UTC+1 all year (pos-core `time::ALGIERS_OFFSET_MS`). */
const ALGIERS_OFFSET_MS = 3_600_000;
const DAY = /^(\d{4})-(\d{2})-(\d{2})$/;

export function isDay(value: string): boolean {
  const match = DAY.exec(value);
  if (!match) return false;
  const [, y, m, d] = match.map(Number) as [number, number, number, number];
  const ms = Date.UTC(y, m - 1, d);
  return new Date(ms).getUTCMonth() === m - 1 && new Date(ms).getUTCDate() === d;
}

/** A `YYYY-MM-DD` day → the UTC midnight that starts it. */
function toUtc(day: string): number {
  const match = DAY.exec(day);
  if (!match) throw new RangeError(`not a YYYY-MM-DD day: ${day}`);
  const [, y, m, d] = match.map(Number) as [number, number, number, number];
  return Date.UTC(y, m - 1, d);
}

function fromUtc(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

function addDays(day: string, days: number): string {
  return fromUtc(toUtc(day) + days * DAY_MS);
}

/** Days from `from` to `to`, both counted (`2026-09-01`…`2026-09-30`: 30). */
export function daysBetween(from: string, to: string): number {
  return Math.round((toUtc(to) - toUtc(from)) / DAY_MS) + 1;
}

/** The instant `formatDate` shows as this day in Algiers (noon, far from either midnight). */
export function dayToMs(day: string): number {
  return toUtc(day) + 12 * 3_600_000 - ALGIERS_OFFSET_MS;
}

/** Today's business date: the calendar day in Algiers, like pos-core `time::business_date`. */
export function today(now: number = Date.now()): string {
  return fromUtc(now + ALGIERS_OFFSET_MS);
}

/** The days a preset covers on the day `on`. Weeks start on Sunday, like the reports; `custom`
 * starts as that day alone. */
export function rangeOf(preset: RangePreset, on: string): DateRange {
  const start = new Date(toUtc(on));
  const year = start.getUTCFullYear();
  const month = start.getUTCMonth();
  const sunday = addDays(on, -start.getUTCDay());
  switch (preset) {
    case "today":
    case "custom":
      return { from: on, to: on };
    case "yesterday": {
      const day = addDays(on, -1);
      return { from: day, to: day };
    }
    case "this_week":
      return { from: sunday, to: on };
    case "last_week":
      return { from: addDays(sunday, -7), to: addDays(sunday, -1) };
    case "this_month":
      return { from: fromUtc(Date.UTC(year, month, 1)), to: on };
    case "last_month":
      return {
        from: fromUtc(Date.UTC(year, month - 1, 1)),
        to: fromUtc(Date.UTC(year, month, 1) - DAY_MS),
      };
    case "this_year":
      return { from: fromUtc(Date.UTC(year, 0, 1)), to: on };
  }
}

/** The preset that gives exactly `range` on the day `on`, else `custom`. */
export function presetOf(range: DateRange, on: string, presets: readonly RangePreset[]): RangePreset {
  const found = presets.find((p) => {
    if (p === "custom") return false;
    const r = rangeOf(p, on);
    return r.from === range.from && r.to === range.to;
  });
  return found ?? "custom";
}
