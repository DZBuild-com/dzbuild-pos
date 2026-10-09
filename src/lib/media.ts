import { convertFileSrc } from "@tauri-apps/api/core";

/** Task 6's media names: `products/<uuid>-<version>.jpg` (thumbnail `….thumb.jpg`). */
const SAFE = /^products\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}-\d+(\.thumb)?\.jpg$/;

/** The asset URL of a product photo stored under `<data>/media`, or null for anything else. */
export function mediaUrl(dataDir: string, name: string | null | undefined, thumb = false): string | null {
  if (!name) return null;
  const file = thumb ? name.replace(/\.jpg$/, ".thumb.jpg") : name;
  if (!SAFE.test(file)) return null;
  const sep = dataDir.includes("\\") ? "\\" : "/";
  return convertFileSrc([dataDir, "media", ...file.split("/")].join(sep));
}
