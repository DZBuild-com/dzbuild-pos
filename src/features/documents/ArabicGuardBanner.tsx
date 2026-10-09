import { Languages } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { Lang } from "@/lib/i18n";
import { useArabicGuard } from "@/lib/ipc/billing";
import type { GuardInput } from "@/lib/ipc/types";

/**
 * DOC-034 (décret 13-378 art. 7): a consumer document about to be printed without Arabic gets a
 * warning with a button that switches its language — never blocking. The backend decides, per
 * the shop's `documents.arabic_guard` setting; without a warning nothing is shown.
 */
export function ArabicGuardBanner({
  input,
  onSwitch,
}: {
  input: GuardInput;
  onSwitch: (lang: Lang) => void;
}) {
  const { t } = useTranslation();
  const guard = useArabicGuard(input);
  const warning = guard.data;
  if (!warning) return null;
  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-3 rounded-control border border-warn bg-warn-soft px-3 py-2 text-warn"
    >
      <Languages aria-hidden className="size-5 shrink-0" />
      <span className="min-w-0 flex-1">{t("billing:guard.text")}</span>
      <Button variant="outline" onClick={() => onSwitch(warning.suggested)}>
        {t("billing:guard.switch")}
      </Button>
    </div>
  );
}
