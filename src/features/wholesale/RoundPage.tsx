import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  FileText,
  GripVertical,
  PackageMinus,
  PackagePlus,
  Plus,
  Truck,
  X,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Splash } from "@/components/splash";
import { StatTile } from "@/components/stat-tile";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { useLocations } from "@/features/stock/stock-parts";
import { dayToMs } from "@/lib/dates";
import { formatDZD, formatDate, formatNumber } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import type { Permission, Round, RoundStatus, Stop } from "@/lib/ipc/types";
import {
  useClosePreview,
  useCloseRound,
  useRound,
  useSetStops,
  useStartRound,
  useUnloadVan,
  useVanStock,
} from "@/lib/ipc/wholesale";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { RouteSheetDialog } from "./RouteSheetDialog";
import { StopPicker } from "./StopPicker";
import { StopReportDialog } from "./StopReportDialog";
import { VanDialog } from "./VanDialog";

type Open = "pick" | "sheet" | "load" | "close" | null;

const STATUS_TONES: Record<RoundStatus, "neutral" | "info" | "ok"> = {
  planned: "neutral",
  out: "info",
  closed: "ok",
};
const STOP_TONES: Record<Stop["status"], "neutral" | "ok" | "danger"> = {
  pending: "neutral",
  delivered: "ok",
  failed: "danger",
};

/** `list` with the item at `from` moved to `to`. */
function moved<T>(list: readonly T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  if (item !== undefined) next.splice(to, 0, item);
  return next;
}

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link
      to="/rounds"
      className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("wholesale:rounds.title")}
    </Link>
  );
}

export function RoundStatusBadge({ status }: { status: RoundStatus }) {
  const { t } = useTranslation();
  return <Badge tone={STATUS_TONES[status]}>{t(`wholesale:rounds.status.${status}`)}</Badge>;
}

/**
 * A delivery round (VRT-100, DOC-030), its panel decided by its status. Planned: the stops picked
 * among the delivery notes and invoices still owed, ordered by drag or the arrow buttons, the
 * route sheet, the van loaded, "Départ". Out: each stop reported delivered or not with what was
 * collected, then "Clôturer la tournée" — the backend's summary (collected by method, the cash to
 * hand in with its droit de timbre, the stops not delivered) before the close books every
 * collection as a versement and prints its `RV…` receipt. Closed: read-only with the receipts'
 * numbers, and "Décharger le camion" while the van still holds stock. Every figure is the
 * backend's.
 */
export function RoundPage({ id }: { id: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const permissions = useSession().data?.permissions ?? [];
  const can = (p: Permission) => permissions.includes(p);
  const round = useRound(id);
  const { nameOf } = useLocations();
  const [open, setOpen] = useState<Open>(null);
  const [stop, setStop] = useState<Stop | null>(null);
  const start = useStartRound();

  if (round.error) {
    return (
      <div className="flex flex-col gap-4">
        <BackLink />
        <p role="alert" className="text-danger">
          {errorText(round.error)}
        </p>
      </div>
    );
  }
  if (!round.data) return <Splash inline />;
  const data = round.data;
  const van = data.vanLocationId ? nameOf(data.vanLocationId) || (data.vanLocationName ?? "") : null;
  const delivers = can("documents.delivery");
  const transfers = can("stock.transfer") && data.vanLocationId !== null;

  return (
    <div className="flex flex-col gap-6">
      <BackLink />
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-3xl font-bold">
          <bdi dir="ltr">{data.number}</bdi>
        </h1>
        <RoundStatusBadge status={data.status} />
      </div>
      <p className="flex flex-wrap gap-x-4 gap-y-1 text-ink-2">
        <span>{t("wholesale:rounds.on", { date: formatDate(dayToMs(data.businessDate), lang) })}</span>
        <span>{t("wholesale:rounds.driverIs", { name: data.driverName })}</span>
        {data.vehiclePlate ? (
          <span>{t("wholesale:rounds.plateIs", { plate: data.vehiclePlate })}</span>
        ) : null}
        <span className="inline-flex items-center gap-1">
          <Truck aria-hidden className="size-4 rtl:-scale-x-100" />
          {van ?? t("wholesale:rounds.noVan")}
        </span>
      </p>

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <StatTile label={t("wholesale:rounds.stops")} value={formatNumber(data.stops.length, lang)} />
        <StatTile label={t("wholesale:rounds.totalDue")} value={<Amount value={data.totalDue} />} />
        <StatTile
          label={t("wholesale:rounds.totalCollected")}
          value={<Amount value={data.totalCollected} />}
        />
        <StatTile label={t("wholesale:rounds.cashCollected")} value={<Amount value={data.cashCollected} />} />
      </div>

      {data.status !== "closed" ? (
        <div className="flex flex-wrap gap-2">
          {data.status === "planned" && delivers ? (
            <Button variant="outline" onClick={() => setOpen("pick")}>
              <Plus aria-hidden />
              {t("wholesale:rounds.pick")}
            </Button>
          ) : null}
          {delivers && data.stops.length > 0 ? (
            <Button variant="outline" onClick={() => setOpen("sheet")}>
              <FileText aria-hidden />
              {t("wholesale:rounds.routeSheet")}
            </Button>
          ) : null}
          {transfers ? (
            <Button variant="outline" onClick={() => setOpen("load")}>
              <PackagePlus aria-hidden />
              {t("wholesale:rounds.load")}
            </Button>
          ) : null}
          {data.status === "planned" && delivers ? (
            <Button
              variant="primary"
              disabled={data.stops.length === 0 || start.isPending}
              onClick={() =>
                start.mutate(data.id, {
                  onSuccess: (r) => toast.success(t("wholesale:rounds.started", { number: r.number })),
                  onError: (error) => toast.error(errorText(error)),
                })
              }
            >
              <Truck aria-hidden className="rtl:-scale-x-100" />
              {t("wholesale:rounds.start")}
            </Button>
          ) : null}
          {data.status === "out" && can("credit.collect") ? (
            <Button variant="primary" onClick={() => setOpen("close")}>
              {t("wholesale:rounds.close")}
            </Button>
          ) : null}
        </div>
      ) : null}

      {data.status === "planned" ? (
        <PlannedStops round={data} editable={delivers} />
      ) : data.status === "out" ? (
        <OutStops round={data} onStop={delivers ? setStop : undefined} />
      ) : (
        <ClosedRound round={data} unloads={transfers} />
      )}

      {open === "pick" ? <AddStops round={data} onClose={() => setOpen(null)} /> : null}
      {open === "sheet" ? (
        <RouteSheetDialog roundId={data.id} number={data.number} onClose={() => setOpen(null)} />
      ) : null}
      {open === "load" && van !== null ? (
        <VanDialog roundId={data.id} van={van} onClose={() => setOpen(null)} />
      ) : null}
      {open === "close" ? <CloseDialog round={data} onClose={() => setOpen(null)} /> : null}
      {stop ? <StopReportDialog roundId={data.id} stop={stop} onClose={() => setStop(null)} /> : null}
    </div>
  );
}

/** The picker of the documents to deliver; the ticked ones join the round after its stops. */
function AddStops({ round, onClose }: { round: Round; onClose: () => void }) {
  const errorText = useErrorText();
  const setStops = useSetStops();
  return (
    <StopPicker
      round={round}
      pending={setStops.isPending}
      onClose={onClose}
      onPick={(ids) =>
        setStops.mutate(
          { id: round.id, documentIds: [...round.stops.map((s) => s.documentId), ...ids] },
          { onSuccess: onClose, onError: (error) => toast.error(errorText(error)) },
        )
      }
    />
  );
}

/** A planned round's stops in route order: dragged, or moved with the arrows, or removed; every
 * change is the new order sent to the backend. */
function PlannedStops({ round, editable }: { round: Round; editable: boolean }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const setStops = useSetStops();
  const [dragged, setDragged] = useState<number | null>(null);
  const ids = round.stops.map((s) => s.documentId);
  const send = (documentIds: string[]) =>
    setStops.mutate({ id: round.id, documentIds }, { onError: (error) => toast.error(errorText(error)) });
  if (round.stops.length === 0) return <p className="text-ink-2">{t("wholesale:rounds.noStops")}</p>;
  return (
    <section className="flex flex-col gap-3">
      {editable ? <p className="text-sm text-ink-2">{t("wholesale:rounds.orderHint")}</p> : null}
      <ol aria-label={t("wholesale:rounds.stopsLabel")} className="flex flex-col gap-2">
        {round.stops.map((s, index) => (
          <li
            key={s.documentId}
            draggable={editable && !setStops.isPending}
            onDragStart={() => setDragged(index)}
            onDragEnd={() => setDragged(null)}
            onDragOver={(event) => {
              if (dragged !== null) event.preventDefault();
            }}
            onDrop={(event) => {
              event.preventDefault();
              if (dragged !== null && dragged !== index) send(moved(ids, dragged, index));
              setDragged(null);
            }}
            className={cn(
              "flex flex-wrap items-center gap-3 rounded-card border border-line bg-surface p-3",
              dragged === index && "opacity-50",
            )}
          >
            {editable ? <GripVertical aria-hidden className="size-5 cursor-grab text-ink-2" /> : null}
            <StopSummary stop={s} />
            {editable ? (
              <span className="ms-auto flex gap-1">
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("wholesale:rounds.up", { seq: s.seq })}
                  disabled={index === 0 || setStops.isPending}
                  onClick={() => send(moved(ids, index, index - 1))}
                >
                  <ArrowUp aria-hidden />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("wholesale:rounds.down", { seq: s.seq })}
                  disabled={index === ids.length - 1 || setStops.isPending}
                  onClick={() => send(moved(ids, index, index + 1))}
                >
                  <ArrowDown aria-hidden />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("wholesale:rounds.removeStop", { seq: s.seq })}
                  disabled={setStops.isPending}
                  onClick={() => send(ids.filter((d) => d !== s.documentId))}
                >
                  <X aria-hidden />
                </Button>
              </span>
            ) : null}
          </li>
        ))}
      </ol>
    </section>
  );
}

/** A stop's number, customer, document, address and what the customer owes on it. */
function StopSummary({ stop }: { stop: Stop }) {
  const { t } = useTranslation();
  return (
    <span className="flex min-w-0 flex-1 flex-wrap items-center gap-x-4 gap-y-1 text-start">
      <span className="grid size-9 shrink-0 place-items-center rounded-full bg-brand-soft font-bold tabular-nums">
        {stop.seq}
      </span>
      <span className="flex min-w-0 flex-col">
        <span className="font-semibold" dir="auto">
          {stop.customerName ?? ""}
        </span>
        <span className="text-sm text-ink-2">
          <bdi dir="ltr">{stop.documentNumber ?? ""}</bdi> · {t(`documents:types.${stop.docType}`)}
          {stop.address ? (
            <>
              {" · "}
              <span dir="auto">{stop.address}</span>
            </>
          ) : null}
        </span>
      </span>
      <span className="ms-auto flex flex-col items-end">
        <span className="text-sm text-ink-2">{t("wholesale:rounds.amountDue")}</span>
        <Amount value={stop.amountDue} className="font-semibold" />
      </span>
    </span>
  );
}

/** A round on the road: each stop is a button opening its report. */
function OutStops({ round, onStop }: { round: Round; onStop?: (stop: Stop) => void }) {
  const { t } = useTranslation();
  return (
    <ol aria-label={t("wholesale:rounds.stopsLabel")} className="flex flex-col gap-2">
      {round.stops.map((s) => {
        const content = (
          <>
            <StopSummary stop={s} />
            <span className="flex flex-col items-end gap-1">
              <Badge tone={STOP_TONES[s.status]}>{t(`wholesale:rounds.stopState.${s.status}`)}</Badge>
              {s.collected > 0 ? (
                <span className="text-sm">
                  <Amount value={s.collected} className="font-semibold" />
                  {s.method ? ` · ${t(`payments:tenders.${s.method}`)}` : null}
                </span>
              ) : null}
            </span>
          </>
        );
        const label = t("wholesale:rounds.stop", {
          seq: s.seq,
          customer: s.customerName ?? s.documentNumber ?? "",
        });
        return (
          <li key={s.documentId}>
            {onStop ? (
              <button
                type="button"
                aria-label={label}
                onClick={() => onStop(s)}
                className="flex w-full flex-wrap items-center gap-3 rounded-card border border-line bg-surface p-3 hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
              >
                {content}
              </button>
            ) : (
              <div className="flex flex-wrap items-center gap-3 rounded-card border border-line bg-surface p-3">
                {content}
              </div>
            )}
          </li>
        );
      })}
    </ol>
  );
}

/** A closed round, read only: every stop with what it brought and its receipt's number; the van
 * unloaded while it still holds stock. */
function ClosedRound({ round, unloads }: { round: Round; unloads: boolean }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const stock = useVanStock(unloads ? round.vanLocationId : null);
  const unload = useUnloadVan();
  const holds = (stock.data ?? []).some((row) => row.qty > 0);
  return (
    <div className="flex flex-col gap-4">
      {unloads && holds ? (
        <div className="flex flex-wrap items-center gap-3 rounded-card bg-warn-soft p-4 text-warn">
          <p className="font-semibold">{t("wholesale:rounds.vanHolds")}</p>
          <Button
            variant="outline"
            className="ms-auto"
            disabled={unload.isPending}
            onClick={() =>
              unload.mutate(round.id, {
                onSuccess: (done) =>
                  toast.success(
                    done
                      ? t("wholesale:rounds.unloaded", { number: done.document.number ?? "" })
                      : t("wholesale:rounds.vanEmpty"),
                  ),
                onError: (error) => toast.error(errorText(error)),
              })
            }
          >
            <PackageMinus aria-hidden />
            {t("wholesale:rounds.unload")}
          </Button>
        </div>
      ) : null}
      <div className="overflow-x-auto rounded-card border border-line bg-surface">
        <table aria-label={t("wholesale:rounds.stopsLabel")} className="w-full border-collapse">
          <thead className="bg-surface-2">
            <tr className="border-b border-line text-sm text-ink-2">
              <th scope="col" className="px-3 py-2 text-start font-semibold">
                {t("wholesale:rounds.number")}
              </th>
              <th scope="col" className="px-3 py-2 text-start font-semibold">
                {t("wholesale:rounds.customer")}
              </th>
              <th scope="col" className="px-3 py-2 text-start font-semibold">
                {t("wholesale:rounds.document")}
              </th>
              <th scope="col" className="px-3 py-2 text-start font-semibold">
                {t("wholesale:rounds.state")}
              </th>
              <th scope="col" className="px-3 py-2 text-end font-semibold">
                {t("wholesale:rounds.amountDue")}
              </th>
              <th scope="col" className="px-3 py-2 text-end font-semibold">
                {t("wholesale:rounds.collected")}
              </th>
              <th scope="col" className="px-3 py-2 text-start font-semibold">
                {t("wholesale:rounds.receipt")}
              </th>
            </tr>
          </thead>
          <tbody>
            {round.stops.map((s) => (
              <tr key={s.documentId} className="border-b border-line even:bg-surface-2">
                <td className="px-3 py-2 tabular-nums">{s.seq}</td>
                <td className="px-3 py-2" dir="auto">
                  {s.customerName ?? ""}
                </td>
                <td className="px-3 py-2">
                  <bdi dir="ltr">{s.documentNumber ?? ""}</bdi>
                </td>
                <td className="px-3 py-2">
                  <Badge tone={STOP_TONES[s.status]}>{t(`wholesale:rounds.stopState.${s.status}`)}</Badge>
                </td>
                <td className="px-3 py-2 text-end">
                  <Amount value={s.amountDue} />
                </td>
                <td className="px-3 py-2 text-end">
                  <Amount value={s.collected} />
                  {s.method ? (
                    <span className="block text-sm text-ink-2">{t(`payments:tenders.${s.method}`)}</span>
                  ) : null}
                </td>
                <td className="px-3 py-2">
                  {s.receiptNumber ? <bdi dir="ltr">{s.receiptNumber}</bdi> : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** The close confirmed after the backend's summary: what was collected by method, the cash the
 * driver hands in (his customers' droit de timbre on cash included), the stops not delivered. */
function CloseDialog({ round, onClose }: { round: Round; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const preview = useClosePreview(round.id);
  const close = useCloseRound();
  const data = preview.data;
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.closeTitle", { number: round.number })}
      description={t("wholesale:rounds.closeHint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!data || close.isPending}
            onClick={() =>
              close.mutate(round.id, {
                onSuccess: (done) => {
                  toast.success(
                    t("wholesale:rounds.closed", {
                      number: done.round.round.number,
                      count: formatNumber(done.round.versements.length, lang),
                    }),
                  );
                  if (!done.printQueued) toast.warning(t("wholesale:rounds.printFailed"));
                  onClose();
                },
              })
            }
          >
            {t("wholesale:rounds.confirmClose")}
          </Button>
        </>
      }
    >
      {data ? (
        <div className="flex flex-col gap-4">
          <section className="flex flex-col gap-2">
            <h3 className="font-semibold">{t("wholesale:rounds.collectedBy")}</h3>
            {data.collected.length === 0 ? (
              <p className="text-ink-2">{t("wholesale:rounds.nothingCollected")}</p>
            ) : (
              <dl className="flex flex-col gap-1">
                {data.collected.map(([method, amount]) => (
                  <div key={method} className="flex justify-between gap-3">
                    <dt>{t(`payments:tenders.${method}`)}</dt>
                    <dd>
                      <Amount value={amount} />
                    </dd>
                  </div>
                ))}
              </dl>
            )}
          </section>
          <p role="status" className="rounded-card bg-brand-soft p-4 text-2xl font-bold">
            {t("wholesale:rounds.cashToHandIn", { amount: formatDZD(data.cashToHandIn, lang) })}
          </p>
          {data.failed > 0 ? (
            <p className="font-semibold text-danger">
              {t("wholesale:rounds.failedStops", { count: formatNumber(data.failed, lang) })}
            </p>
          ) : null}
        </div>
      ) : preview.error ? (
        <p role="alert" className="text-danger">
          {errorText(preview.error)}
        </p>
      ) : (
        <p className="text-ink-2">{t("common:status.loading")}</p>
      )}
      {close.error ? (
        <p role="alert" className="text-danger">
          {errorText(close.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
