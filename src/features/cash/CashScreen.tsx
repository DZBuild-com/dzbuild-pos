import {
  ArrowDownToLine,
  ArrowUpFromLine,
  FileBarChart,
  Landmark,
  Lock,
  Receipt,
  Wallet,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";
import { Amount } from "@/components/amount";
import { EmptyState } from "@/components/empty-state";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { FlexyScreen } from "@/features/phones/FlexyScreen";
import { formatDate } from "@/lib/format";
import { useUserTiles } from "@/lib/ipc/auth";
import {
  useCashMovements,
  useCashSessions,
  useCurrentSession,
  usePrintX,
  usePrintZ,
  useXReport,
  useZReport,
} from "@/lib/ipc/cash";
import { useModule } from "@/lib/ipc/pack-state";
import type { ManualKind, Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { CashMoveDialog } from "./CashMoveDialog";
import { CloseSessionDialog } from "./CloseSessionDialog";
import { ExpenseDialog } from "./ExpenseDialog";
import { ExpensesTab } from "./ExpensesTab";
import { OpenSessionDialog } from "./OpenSessionDialog";
import { ReportView } from "./ReportView";

type Open =
  | { kind: "move"; move: ManualKind }
  | { kind: "expense" }
  | { kind: "x" }
  | { kind: "close" }
  | { kind: "z"; sessionId: string }
  | null;

type Tab = "session" | "expenses" | "flexy";

/** `/cash`: the cash session of this terminal and, for whoever reads them (`reports.cash` or
 * `cash.view_history`), the expenses (FIN-001/002); with the flexy module on, the top-up float
 * (VRT-002) for whoever buys it or reads the cash reports. The tab is kept in `?tab=`. */
export function CashScreen({ session }: { session: Session }) {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const can = (p: Session["permissions"][number]) => session.permissions.includes(p);
  const readsExpenses = can("reports.cash") || can("cash.view_history");
  const flexy = useModule("flexy") && (can("cash.paid_in_out") || can("reports.cash"));
  const tabs: Tab[] = [
    "session",
    ...(readsExpenses ? (["expenses"] as const) : []),
    ...(flexy ? (["flexy"] as const) : []),
  ];
  const tab = tabs.find((k) => k === params.get("tab")) ?? "session";
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("cash:title")}</h1>
      {tabs.length === 1 ? (
        <SessionPanel session={session} />
      ) : (
        <>
          <div
            role="tablist"
            aria-label={t("cash:tabs.label")}
            className="flex flex-wrap gap-2 border-b border-line pb-2"
          >
            {tabs.map((k) => (
              <button
                key={k}
                type="button"
                role="tab"
                id={`cash-tab-${k}`}
                aria-selected={tab === k}
                aria-controls={`cash-panel-${k}`}
                onClick={() => setParams({ tab: k }, { replace: true })}
                className={cn(
                  "min-h-11 rounded-control px-4 font-semibold",
                  tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
                )}
              >
                {t(`cash:tabs.${k}`)}
              </button>
            ))}
          </div>
          <div role="tabpanel" id={`cash-panel-${tab}`} aria-labelledby={`cash-tab-${tab}`}>
            {tab === "session" ? (
              <SessionPanel session={session} />
            ) : tab === "expenses" ? (
              <ExpensesTab />
            ) : (
              <FlexyScreen />
            )}
          </div>
        </>
      )}
    </div>
  );
}

/** The cash session of this terminal: open it, move money, X report, close with the Z, and the
 * closed sessions for whoever reads the history. */
function SessionPanel({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const current = useCurrentSession();
  const tiles = useUserTiles();
  const [opening, setOpening] = useState(false);
  const [open, setOpen] = useState<Open>(null);
  const can = (p: Session["permissions"][number]) => session.permissions.includes(p);
  const till = current.data ?? null;
  // Every drawer movement adds up to the expected cash: never shown to a blind closer (CSH-004).
  const seesDrawer = can("cash.view_expected") || can("cash.view_history");
  const movements = useCashMovements(seesDrawer ? (till?.id ?? null) : null);
  const history = useCashSessions(can("cash.view_history"));
  const nameOf = (id: string | null) => tiles.data?.find((u) => u.id === id)?.displayName ?? "";
  const closedSessions = (history.data ?? []).filter((s) => s.status === "closed");

  return (
    <div className="flex flex-col gap-6">
      {current.isPending ? null : till === null ? (
        <EmptyState
          icon={Lock}
          title={t("cash:closedTitle")}
          body={t("cash:closedHint")}
          action={
            can("cash.open") ? (
              <Button variant="primary" size="lg" onClick={() => setOpening(true)}>
                <Wallet aria-hidden />
                {t("cash:open.button")}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <Card>
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <p className="text-xl font-bold">{t("cash:status.open")}</p>
            <p className="text-ink-2">
              {t("cash:openedBy", { name: nameOf(till.userId) })}{" "}
              <bdi dir="ltr">{formatDate(till.openedAt, lang, { withTime: true })}</bdi>
            </p>
          </div>
          {seesDrawer ? (
            <p className="flex items-center justify-between text-lg">
              <span>{t("cash:report.float")}</span>
              <Amount value={till.openingFloat} className="font-bold" />
            </p>
          ) : null}
          <div className="flex flex-wrap gap-2">
            {can("cash.paid_in_out") ? (
              <>
                <Button variant="secondary" onClick={() => setOpen({ kind: "move", move: "paid_in" })}>
                  <ArrowDownToLine aria-hidden />
                  {t("cash:move.paid_in.button")}
                </Button>
                <Button variant="secondary" onClick={() => setOpen({ kind: "move", move: "paid_out" })}>
                  <ArrowUpFromLine aria-hidden />
                  {t("cash:move.paid_out.button")}
                </Button>
                <Button variant="secondary" onClick={() => setOpen({ kind: "move", move: "drop" })}>
                  <Landmark aria-hidden />
                  {t("cash:move.drop.button")}
                </Button>
                <Button variant="secondary" onClick={() => setOpen({ kind: "expense" })}>
                  <Receipt aria-hidden />
                  {t("cash:expense.button")}
                </Button>
              </>
            ) : null}
            {can("cash.close") ? (
              <>
                <Button variant="outline" onClick={() => setOpen({ kind: "x" })}>
                  <FileBarChart aria-hidden />
                  {t("cash:report.xButton")}
                </Button>
                <Button variant="primary" onClick={() => setOpen({ kind: "close" })}>
                  {t("cash:close.button")}
                </Button>
              </>
            ) : null}
          </div>
          {seesDrawer ? <MovementsTable rows={movements.data ?? []} /> : null}
        </Card>
      )}
      {can("cash.view_history") ? (
        <Card>
          <h2 className="text-xl font-bold">{t("cash:history.title")}</h2>
          {closedSessions.length === 0 ? (
            <p className="text-ink-2">{t("cash:history.empty")}</p>
          ) : (
            <table className="w-full border-collapse">
              <thead className="text-sm text-ink-2">
                <tr className="h-10">
                  <th className="text-start font-semibold">{t("cash:history.z")}</th>
                  <th className="text-start font-semibold">{t("cash:history.date")}</th>
                  <th className="text-end font-semibold">{t("cash:report.variance")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {closedSessions.map((s) => (
                  <tr key={s.id} className="h-10 border-t border-line even:bg-surface-2">
                    <td>
                      <bdi dir="ltr">{s.zNumber}</bdi>
                    </td>
                    <td>
                      <bdi dir="ltr">{formatDate(s.closedAt ?? s.openedAt, lang, { withTime: true })}</bdi>
                    </td>
                    <td className="text-end">{s.variance !== null ? <Amount value={s.variance} /> : null}</td>
                    <td className="text-end">
                      <Button variant="ghost" onClick={() => setOpen({ kind: "z", sessionId: s.id })}>
                        {t("cash:history.show")}
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Card>
      ) : null}
      <OpenSessionDialog open={opening} onOpenChange={setOpening} />
      {open?.kind === "move" ? <CashMoveDialog kind={open.move} onClose={() => setOpen(null)} /> : null}
      {open?.kind === "expense" ? <ExpenseDialog onClose={() => setOpen(null)} /> : null}
      {open?.kind === "close" ? <CloseSessionDialog session={session} onClose={() => setOpen(null)} /> : null}
      {open?.kind === "x" ? <XDialog onClose={() => setOpen(null)} errorText={errorText} /> : null}
      {open?.kind === "z" ? (
        <ZDialog sessionId={open.sessionId} onClose={() => setOpen(null)} errorText={errorText} />
      ) : null}
    </div>
  );
}

function MovementsTable({
  rows,
}: {
  rows: { id: string; kind: string; amount: number; note: string | null; createdAt: number }[];
}) {
  const { t } = useTranslation();
  const lang = useLang();
  if (rows.length === 0) return <p className="text-ink-2">{t("cash:movements.empty")}</p>;
  return (
    <table className="w-full border-collapse">
      <thead className="text-sm text-ink-2">
        <tr className="h-10">
          <th className="text-start font-semibold">{t("cash:movements.time")}</th>
          <th className="text-start font-semibold">{t("cash:movements.kind")}</th>
          <th className="text-start font-semibold">{t("cash:movements.note")}</th>
          <th className="text-end font-semibold">{t("cash:move.amount")}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((m) => (
          <tr key={m.id} className="h-10 border-t border-line even:bg-surface-2">
            <td>
              <bdi dir="ltr">{formatDate(m.createdAt, lang, { withTime: true })}</bdi>
            </td>
            <td>{t(`cash:movements.kinds.${m.kind}`)}</td>
            <td>{m.note}</td>
            <td className="text-end">
              <Amount value={m.amount} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function XDialog({ onClose, errorText }: { onClose: () => void; errorText: (e: unknown) => string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const x = useXReport(true);
  const printX = usePrintX();
  return (
    <Dialog open size="lg" onOpenChange={(o) => (o ? undefined : onClose())} title={t("cash:report.x")}>
      {x.data ? (
        <ReportView summary={x.data} onPrint={() => printX.mutate(lang)} printing={printX.isPending} />
      ) : null}
      {x.error ? (
        <p role="alert" className="text-danger">
          {errorText(x.error)}
        </p>
      ) : null}
      {printX.error ? (
        <p role="alert" className="text-danger">
          {errorText(printX.error)}
        </p>
      ) : null}
    </Dialog>
  );
}

function ZDialog({
  sessionId,
  onClose,
  errorText,
}: {
  sessionId: string;
  onClose: () => void;
  errorText: (e: unknown) => string;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const z = useZReport(sessionId);
  const printZ = usePrintZ();
  return (
    <Dialog open size="lg" onOpenChange={(o) => (o ? undefined : onClose())} title={t("cash:report.z")}>
      {z.data ? (
        <ReportView
          summary={z.data.summary}
          z={z.data}
          onPrint={() => printZ.mutate({ sessionId, lang })}
          printing={printZ.isPending}
        />
      ) : null}
      {printZ.error ? (
        <p role="alert" className="text-danger">
          {errorText(printZ.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
