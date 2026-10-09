import type { Module, PackRelease, ShopActivity, TillMode } from "@/lib/ipc/types";

/** pos-core `Module::ALL`: the order `packs::state` sorts the modules in. */
export const ALL_MODULES: readonly Module[] = [
  "restaurant",
  "department_keys",
  "regulated_prices",
  "lots",
  "variant_grid",
  "yield",
  "waste",
  "pharmacy",
  "serials",
  "flexy",
  "repairs",
  "instalments",
  "wholesale",
  "delivery_rounds",
  "orders",
  "appointments",
  "loyalty",
  "gift_vouchers",
  "jewellery",
  "staff",
  "zakat",
];

export type ModuleGroup =
  "sale" | "restaurant" | "health" | "services" | "credit" | "wholesale" | "loyalty" | "jewellery" | "staff";

/** Settings › Activité et modules: the 21 switches under their group (`packs:groups.*`), each
 * module once. */
export const MODULE_GROUPS: readonly { group: ModuleGroup; modules: readonly Module[] }[] = [
  {
    group: "sale",
    modules: ["department_keys", "regulated_prices", "lots", "variant_grid", "serials", "flexy"],
  },
  { group: "restaurant", modules: ["restaurant"] },
  { group: "health", modules: ["pharmacy"] },
  { group: "services", modules: ["repairs", "appointments", "yield", "waste"] },
  { group: "credit", modules: ["instalments", "orders"] },
  { group: "wholesale", modules: ["wholesale", "delivery_rounds"] },
  { group: "loyalty", modules: ["loyalty", "gift_vouchers"] },
  { group: "jewellery", modules: ["jewellery"] },
  { group: "staff", modules: ["staff", "zakat"] },
];

/** The four till screens, `packs:tillModes.*`. */
export const TILL_MODES: readonly TillMode[] = ["retail", "restaurant", "wholesale", "mixed"];

/** The template releases under "Prêts à l'emploi" (the others are "Bientôt enrichis": their
 * modules work, their extra seeds arrive later). */
export const READY_RELEASES: readonly PackRelease[] = ["mvp"];

/** ONB-009: the activities whose setup wizard offers the scale step after the printer. */
export const SCALE_ACTIVITIES: readonly ShopActivity[] = ["butcher", "general", "bakery"];

/** `modules` with `module` turned on or off, in pos-core's order. */
export function withModule(modules: readonly Module[], module: Module, on: boolean): Module[] {
  return ALL_MODULES.filter((m) => (m === module ? on : modules.includes(m)));
}
