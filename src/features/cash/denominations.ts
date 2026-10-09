/** Algerian notes and coins in centimes, mirroring pos-core `cash::closing::DENOMINATIONS`
 * (2 000 … 5 DA); the backend refuses any other (`count_invalid`). */
export const DENOMINATIONS = [200_000, 100_000, 50_000, 20_000, 10_000, 5_000, 2_000, 1_000, 500] as const;

/** 200 DA and above are notes in the count sheet's labels (200 DA also exists as a coin). */
export function isNote(denomination: number): boolean {
  return denomination >= 20_000;
}

export type Counts = Record<number, string>;

export function emptyCounts(): Counts {
  return Object.fromEntries(DENOMINATIONS.map((d) => [d, ""]));
}

/** Pieces counted per denomination, as integers (empty = 0). */
export function countLines(counts: Counts): { denomination: number; count: number }[] {
  return DENOMINATIONS.map((denomination) => ({
    denomination,
    count: Number(counts[denomination] || "0"),
  })).filter((l) => l.count > 0);
}

/** The total the sheet shows while typing (integer centimes; the backend recomputes it). */
export function countedTotal(counts: Counts): number {
  return countLines(counts).reduce((sum, l) => sum + l.denomination * l.count, 0);
}
