import type { TFunction } from "i18next";
import type { Metal } from "@/lib/ipc/types";

export const METALS: readonly Metal[] = ["gold", "silver", "platinum"];

/** The carats of a gold fineness: 750 ‰ → 18, 916 ‰ → 22, 585 ‰ → 14. */
export function carats(titre: number): number {
  return Math.round((titre * 24) / 1000);
}

/** "Or 750 ‰ (18 carats)"; silver and platinum by their fineness alone ("Argent 925 ‰"). */
export function finenessLabel(t: TFunction, metal: Metal, titre: number): string {
  const name = t(`jewellery:metal.${metal}`);
  return metal === "gold"
    ? t("jewellery:fineness", { metal: name, titre, carats: carats(titre) })
    : t("jewellery:finenessPlain", { metal: name, titre });
}
