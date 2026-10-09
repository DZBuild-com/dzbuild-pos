import { useLanStatus, useReplicaRefresh } from "@/lib/ipc/lan";

/** Mounted once per signed-in session: the status stays pushed, and a till's screens follow the
 * main PC's data. */
export function LanWatch() {
  const status = useLanStatus();
  useReplicaRefresh(status.data);
  return null;
}
