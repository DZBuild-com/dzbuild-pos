import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/** "Amine Benali" → "AB"; "ياسين" → "ي"; "Amine (démo)" → "A" (a bracketed part and anything
 * but a letter or a digit are skipped: never ")A" on a tile). */
export function initials(name: string): string {
  return name
    .replace(/\([^)]*\)/gu, " ")
    .trim()
    .split(/\s+/)
    .map((word) => word.match(/[\p{L}\p{N}]/u)?.[0] ?? "")
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toLocaleUpperCase();
}
