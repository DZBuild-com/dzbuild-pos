import { lazy } from "react";
import { loadBackOffice } from "@/lib/i18n";

/** The till's pack extensions (`till-packs.tsx`) are a chunk of their own, read with
 * the back office's texts (the packs' texts live there) the first time a pack module needs it;
 * the till's entry holds only these slots. */
export const loadTillPacks = () =>
  Promise.all([import("./till-packs"), loadBackOffice()]).then(([chunk]) => chunk);

export const PackDialog = lazy(() => loadTillPacks().then((m) => ({ default: m.PackDialog })));
export const DepartmentKeysPanel = lazy(() =>
  loadTillPacks().then((m) => ({ default: m.DepartmentKeysPanel })),
);
export const VanBanner = lazy(() => loadTillPacks().then((m) => ({ default: m.VanBanner })));
export const LineNotices = lazy(() => loadTillPacks().then((m) => ({ default: m.LineNotices })));
export const PaymentTiles = lazy(() => loadTillPacks().then((m) => ({ default: m.PaymentTiles })));
export const PointsLine = lazy(() => loadTillPacks().then((m) => ({ default: m.PointsLine })));
export const ReturnSerials = lazy(() => loadTillPacks().then((m) => ({ default: m.ReturnSerials })));
