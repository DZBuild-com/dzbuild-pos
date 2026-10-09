import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useDoctor, usePrinterStatus } from "@/lib/ipc/devices";
import type { HardwareConfig, PrinterIssue, PrinterStatus } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

type StatusKey = "ready" | "paperOut" | "paperLow" | "coverOpen" | "offline" | "error";

const TONES: Record<StatusKey, string> = {
  ready: "bg-ok",
  paperLow: "bg-warn",
  paperOut: "bg-danger",
  coverOpen: "bg-danger",
  offline: "bg-danger",
  error: "bg-danger",
};

/** The one thing to tell about a printer, the most blocking first. */
function statusKey(status: PrinterStatus): StatusKey {
  if (!status.reachable) return "offline";
  if (status.paperOut) return "paperOut";
  if (status.coverOpen) return "coverOpen";
  if (status.error) return "error";
  if (status.paperLow) return "paperLow";
  return "ready";
}

function issueText(issue: PrinterIssue, t: (key: string, options: Record<string, string>) => string): string {
  const kind = issue.issue;
  return t(`hardware:doctor.issues.${kind.kind}`, {
    name: issue.printerName,
    saved: kind.kind === "driver_changed" ? kind.saved : "",
    now: kind.kind === "driver_changed" ? kind.now : "",
  });
}

/** "Vérifier les imprimantes" (HW-009): saved printers Windows lost, whose driver Windows Update
 * swapped, or whose queue is offline, in plain words; and each saved printer's paper, cover and
 * reachability (HW-008). The saved `hardware.json` is what the backend checks. */
export function DoctorPanel({ config }: { config: HardwareConfig }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const doctor = useDoctor();
  const printers = [...(config.printers ?? []), ...(config.labelPrinters ?? [])];
  return (
    <Card>
      <div className="flex flex-wrap items-center gap-4">
        <Button variant="outline" disabled={doctor.isPending} onClick={() => doctor.mutate()}>
          {t("hardware:doctor.check")}
        </Button>
        {printers.map((p) => (
          <StatusDot key={p.id} id={p.id} name={p.name} />
        ))}
      </div>
      <div aria-live="polite" className="flex flex-col gap-2">
        {doctor.data?.length === 0 ? <p className="text-ok">{t("hardware:doctor.none")}</p> : null}
        {doctor.data && doctor.data.length > 0 ? (
          <ul className="flex flex-col gap-2">
            {doctor.data.map((issue) => (
              <li
                key={`${issue.printerId}-${issue.issue.kind}`}
                className="rounded-card bg-warn-soft p-3 text-warn"
              >
                {issueText(issue, t)}
              </li>
            ))}
          </ul>
        ) : null}
        {doctor.error ? (
          <p role="alert" className="text-danger">
            {errorText(doctor.error)}
          </p>
        ) : null}
      </div>
    </Card>
  );
}

function StatusDot({ id, name }: { id: string; name: string }) {
  const { t } = useTranslation();
  const status = usePrinterStatus(id);
  if (!status.data) return null;
  const key = statusKey(status.data);
  return (
    <span className="inline-flex items-center gap-2">
      <span aria-hidden className={cn("size-2.5 rounded-full", TONES[key])} />
      <span className="font-semibold">{name}</span>
      <span className="text-ink-2">{t(`hardware:doctor.status.${key}`)}</span>
    </span>
  );
}
