import { RefreshCw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { usePendingEntries, usePendingRetry } from "@/lib/ipc/lan";
import type { PendingKind } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** `sync:lan.pending.services.<key>`: a host service id as a translation key. */
const serviceKey = (service: string) => service.replaceAll(".", "_");

/**
 * On a till (Settings › Réseau): the sales whose host entries (legal registers, IMEIs,
 * top-ups, deposits, vouchers) wait for the main PC, the money calls whose answer was lost, and
 * the main PC's refusals, with « Réessayer ». Never the entries themselves: no patient, prescriber
 * or buyer data leaves the backend.
 */
export function PendingEntriesCard() {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const pending = usePendingEntries();
  const retry = usePendingRetry();
  const data = pending.data;
  if (!data) return null;
  const count = data.entries.length + data.calls.length;
  const serviceName = (service: string) => {
    const key = `sync:lan.pending.services.${serviceKey(service)}`;
    return i18n.exists(key) ? t(key) : t("sync:lan.pending.services.other");
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("sync:lan.pending.title")}</h2>
      <p className="text-ink-2">{t("sync:lan.pending.help")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <Badge tone={data.refused > 0 ? "danger" : count > 0 ? "warn" : "ok"}>
          {t("sync:lan.pending.count", { count })}
        </Badge>
        {data.refusedRegisters > 0 ? (
          <Badge tone="danger">
            <TriangleAlert aria-hidden className="size-4" />
            {t("sync:lan.pending.registersBadge", { count: data.refusedRegisters })}
          </Badge>
        ) : null}
      </div>
      {data.refusedRegisters > 0 ? (
        <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
          {t("sync:lan.pending.registersHelp")}
        </p>
      ) : null}
      {count > 0 ? (
        <ul className="flex flex-col gap-2">
          {data.entries.map((entry) => (
            <li key={entry.documentId} className="flex flex-col gap-1 rounded-control border border-line p-3">
              <p className="font-semibold">
                {t("sync:lan.pending.ticket", {
                  number: entry.number ?? "—",
                  date: formatDate(dayToMs(entry.businessDate), lang),
                })}
              </p>
              <p className="text-ink-2">
                {entry.kinds.map((kind: PendingKind) => t(`sync:lan.pending.kinds.${kind}`)).join(" · ")}
              </p>
              <p className={entry.refused ? "text-danger" : "text-ink-2"}>
                {entry.refused ? errorText({ code: entry.refused }) : t("sync:lan.pending.waiting")}
              </p>
            </li>
          ))}
          {data.calls.map((call) => (
            <li
              key={`${call.service}-${call.at}`}
              className="flex flex-col gap-1 rounded-control border border-line p-3"
            >
              <p className="font-semibold">
                {t("sync:lan.pending.call", {
                  service: serviceName(call.service),
                  date: formatDate(call.at, lang, { withTime: true }),
                })}
              </p>
              <p className={call.refused ? "text-danger" : "text-ink-2"}>
                {call.refused
                  ? errorText({ code: call.refused })
                  : call.answered
                    ? t("sync:lan.pending.answered")
                    : t("sync:lan.pending.waiting")}
              </p>
            </li>
          ))}
        </ul>
      ) : null}
      {count > 0 ? (
        <div>
          <Button variant="outline" disabled={retry.isPending} onClick={() => retry.mutate()}>
            <RefreshCw aria-hidden />
            {t("sync:lan.pending.retry")}
          </Button>
        </div>
      ) : null}
      {retry.error || pending.error ? (
        <p role="alert" className="text-danger">
          {errorText(retry.error ?? pending.error)}
        </p>
      ) : null}
    </Card>
  );
}
