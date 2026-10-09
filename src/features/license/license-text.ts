import type { LicenseStatus } from "@/lib/ipc/types";

/** The `license:state.*` sentence that explains why this PC cannot write, when there is one to say
 * (a PC that never had a licence needs none). */
export function stateNoticeKey(status: LicenseStatus): string | null {
  switch (status.state) {
    case "expired":
      return "license:state.expired";
    case "otherDevice":
      return "license:state.otherDevice";
    case "tampered":
      return "license:state.tampered";
    case "refused":
      return "license:state.refused";
    case "deviceUnreadable":
      return "license:state.deviceUnreadable";
    default:
      return null;
  }
}
