import { Navigate } from "react-router";
import { usePackState } from "@/lib/ipc/pack-state";
import type { Session } from "@/lib/ipc/types";
import { SaleScreen } from "./SaleScreen";

/** UX-020: the till the shop's mode calls for. `/pos` opens the floor in the restaurant mode and
 * the sale screen with a "Tables" button in the mixed mode; `direct` (the floor's "Vente directe")
 * is always the sale screen. */
export function TillEntry({ session, direct = false }: { session: Session; direct?: boolean }) {
  const { data, isPending } = usePackState();
  if (isPending) return null;
  const restaurant = data?.modules.includes("restaurant") ?? false;
  if (!direct && restaurant && data?.tillMode === "restaurant") return <Navigate to="/pos/floor" replace />;
  return <SaleScreen session={session} withTables={restaurant && (direct || data?.tillMode === "mixed")} />;
}
