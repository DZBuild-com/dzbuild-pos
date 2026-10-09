import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useAppClosing } from "@/lib/ipc/app";

/** DAT-024: the app backs up before it closes; the window says so until the process ends. */
export function ClosingOverlay() {
  const { t } = useTranslation();
  const closing = useAppClosing();
  if (!closing) return null;
  return (
    <div
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="closing-title"
      className="fixed inset-0 z-[100] grid place-items-center bg-bg/90"
    >
      <div className="flex items-center gap-3 rounded-card border border-line bg-surface p-6 shadow-overlay">
        <LoaderCircle aria-hidden className="size-6 animate-spin text-ink-2" />
        <p id="closing-title" className="text-lg font-semibold">
          {t("common:closing.saving")}
        </p>
      </div>
    </div>
  );
}
