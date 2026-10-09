import type { LinkState } from "@/lib/ipc/types";

/** The colour of a till's link to its main PC, in the shell's pill and Settings › Réseau. Its own
 * module: the shell reads it without pulling the network settings (and their join wizard) into the
 * till's first chunk (NFR-010). */
export const LINK_TONE: Record<LinkState, "ok" | "warn" | "danger" | "info" | "neutral"> = {
  connecting: "neutral",
  online: "ok",
  offline: "warn",
  replaying: "info",
  behind: "info",
  attention: "danger",
  version_mismatch: "danger",
};
