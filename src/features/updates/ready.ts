import type { Lang } from "@/lib/i18n";
import type { UpdateStatus } from "@/lib/ipc/types";

/** The version to offer for installation now (PLT-011): downloaded and verified, no drawer open
 * and not already installing; else null. */
export function installableVersion(status: UpdateStatus | undefined): string | null {
  if (!status?.offer || status.download.kind !== "ready") return null;
  if (status.blockers.length > 0 || status.installing) return null;
  return status.offer.version;
}

/** The version an offer shows: installable now, or, once "Installer maintenant" was pressed
 * (`pressed`), the offered one, so a refusal (a drawer opened meanwhile, `update_blocked`) stays
 * on screen with its reason instead of the offer vanishing. */
export function offeredVersion(status: UpdateStatus | undefined, pressed: boolean): string | null {
  return installableVersion(status) ?? (pressed ? (status?.offer?.version ?? null) : null);
}

/** A version or a date inside a translated sentence. In Arabic it is isolated left-to-right (the
 * text form of `<bdi dir="ltr">`), so "0.7.0-beta.1" or "27/09/2026 08:00" keep their order; French
 * and English sentences already run left to right. */
export function ltrIn(text: string, lang: Lang): string {
  return lang === "ar" ? `\u2066${text}\u2069` : text;
}
