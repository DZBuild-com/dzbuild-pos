import { lazy } from "react";

// Every pack screen of the back office is a chunk of its own, loaded the first time it
// opens (its module is on); the screens hosting them (products, stock, counts, receipts) stay as
// they were for a shop without the pack. Their texts are the back office's (already loaded).

export const MedicineTab = lazy(() =>
  import("@/features/pharmacy/MedicineTab").then((m) => ({ default: m.MedicineTab })),
);
export const ExpiryBoard = lazy(() =>
  import("@/features/pharmacy/ExpiryBoard").then((m) => ({ default: m.ExpiryBoard })),
);
export const ChifaImport = lazy(() =>
  import("@/features/pharmacy/ChifaImport").then((m) => ({ default: m.ChifaImport })),
);
export const ControlledRegister = lazy(() =>
  import("@/features/pharmacy/ControlledRegister").then((m) => ({ default: m.ControlledRegister })),
);
export const YieldScreen = lazy(() =>
  import("@/features/butcher/YieldScreen").then((m) => ({ default: m.YieldScreen })),
);
export const WasteScreen = lazy(() =>
  import("@/features/butcher/WasteScreen").then((m) => ({ default: m.WasteScreen })),
);
export const VariantGrid = lazy(() =>
  import("@/features/products/VariantGrid").then((m) => ({ default: m.VariantGrid })),
);
export const VariantGridDialog = lazy(() =>
  import("@/features/products/VariantGrid").then((m) => ({ default: m.VariantGridDialog })),
);
export const RegulatedBoard = lazy(() =>
  import("@/features/products/RegulatedBoard").then((m) => ({ default: m.RegulatedBoard })),
);
export const DepartmentKeysEditor = lazy(() =>
  import("@/features/products/DepartmentKeysEditor").then((m) => ({ default: m.DepartmentKeysEditor })),
);
