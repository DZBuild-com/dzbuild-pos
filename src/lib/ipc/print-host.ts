import type { UnlistenFn } from "@tauri-apps/api/event";
import { commands, events } from "./client";
import type { HostJob, RenderResult } from "./types";

/** The print-host page's two commands and its wake-up event (no QueryClient on that page). */
export const printHost = {
  next: (): Promise<HostJob | null> => commands.printHostNext(),
  rendered: (jobId: string, result: RenderResult): Promise<null> => commands.printHostRendered(jobId, result),
  onWake: (cb: () => void): Promise<UnlistenFn> => events.printHostWake.listen(() => cb()),
};
