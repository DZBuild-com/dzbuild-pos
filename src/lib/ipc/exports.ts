import { useMutation } from "@tanstack/react-query";
import { commands } from "./client";
import type { ExportRequest } from "./types";

/** Writes the target as xlsx or csv in the chosen language after a Rust save dialog; resolves
 * `{ path, rows }`, or `null` when the user cancels. */
export function useExport() {
  return useMutation({ mutationFn: (request: ExportRequest) => commands.exportsSave(request) });
}
