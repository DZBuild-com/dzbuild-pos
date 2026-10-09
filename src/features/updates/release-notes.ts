import type { Lang } from "@/lib/i18n";

type Notes = Record<Lang, string[]>;

const files = import.meta.glob<Notes>("../../release-notes/*.json", { import: "default" });

const pathOf = (version: string) => `../../release-notes/${version}.json`;

/** Whether `version` ships its notes (`src/release-notes/<version>.json`), without loading them. */
export function hasReleaseNotes(version: string): boolean {
  return Object.hasOwn(files, pathOf(version));
}

/** The bundled notes of `version` (`src/release-notes/<version>.json`), or null. */
export async function loadReleaseNotes(version: string): Promise<Notes | null> {
  const load = hasReleaseNotes(version) ? files[pathOf(version)] : undefined;
  return load ? load() : null;
}
