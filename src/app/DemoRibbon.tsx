import { FlaskConical } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useDemoStatus, useLeaveDemo } from "@/lib/ipc/demo";
import { useErrorText } from "@/lib/use-error-text";

/** ONB-002: the bar at the top of every screen of the demo shop, with the way back to the real
 * shop (the app restarts on it). Nothing outside the demo. */
export function DemoRibbon() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const status = useDemoStatus();
  const leave = useLeaveDemo();
  if (!status.data?.active) return null;
  return (
    <div
      role="note"
      className="flex flex-wrap items-center gap-3 border-b border-warn bg-warn-soft px-4 py-2 text-ink"
    >
      <FlaskConical aria-hidden className="size-5 shrink-0 text-warn" />
      <p className="min-w-0 flex-1 font-semibold">{t("common:demo.ribbon")}</p>
      {leave.error ? (
        <p role="alert" className="text-danger">
          {errorText(leave.error)}
        </p>
      ) : null}
      <Button
        variant="secondary"
        disabled={leave.isPending || leave.isSuccess}
        onClick={() => leave.mutate()}
      >
        {t("common:demo.leave")}
      </Button>
    </div>
  );
}
