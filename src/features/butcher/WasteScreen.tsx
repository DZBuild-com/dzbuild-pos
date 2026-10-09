import { X } from "lucide-react";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { NativeSelect } from "@/components/ui/native-select";
import { ProductPicker, useUnitDecimals } from "@/features/purchases/purchase-lines";
import { useCan } from "@/features/stock/stock-parts";
import { today } from "@/lib/dates";
import { formatDZD } from "@/lib/format";
import { useRecordWaste, useWasteDay } from "@/lib/ipc/butcher";
import type { WasteLine, WasteReason, WasteRow } from "@/lib/ipc/types";
import { formatQty, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

const REASONS: readonly WasteReason[] = ["damage", "expiry", "loss", "own_use"];

interface Draft {
  productId: string;
  name: string;
  unitCode: string;
  qty: string;
  reason: WasteReason;
}

const col = columnHelper<WasteRow>();

/**
 * STK-026: the day's losses, typed as a grid (a product, its quantity and why: breakage, expiry,
 * loss, own use) and saved as stock adjustments at once; under it, what was saved today with its
 * cost and the total at cost (summed by the backend, shown to whoever sees costs).
 */
export function WasteScreen() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const errorText = useErrorText();
  const id = useId();
  const decimalsOf = useUnitDecimals();
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [invalid, setInvalid] = useState<ReadonlySet<string>>(new Set());
  const [problem, setProblem] = useState<string | null>(null);
  const day = useWasteDay(today());
  const record = useRecordWaste();
  const seesCosts = can("reports.profit");
  const adjusts = can("stock.adjust");

  const patch = (productId: string, change: Partial<Draft>) =>
    setDrafts((all) => all.map((d) => (d.productId === productId ? { ...d, ...change } : d)));
  const save = () => {
    const lines: WasteLine[] = [];
    const wrong = new Set<string>();
    for (const draft of drafts) {
      const qty = parseQty(draft.qty, decimalsOf(draft.unitCode));
      if (qty === null) wrong.add(draft.productId);
      else lines.push({ productId: draft.productId, qty, reason: draft.reason });
    }
    setInvalid(wrong);
    if (lines.length === 0 || wrong.size > 0) {
      setProblem(wrong.size > 0 ? null : t("butcher:waste.nothing"));
      return;
    }
    setProblem(null);
    record.mutate(
      { locationId: null, lines, note: null },
      {
        onSuccess: () => {
          toast.success(t("butcher:waste.saved"));
          setDrafts([]);
        },
      },
    );
  };

  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("name", { header: () => t("butcher:waste.product") }),
        col.accessor("reason", {
          header: () => t("butcher:waste.reason"),
          cell: (c) => t(`butcher:waste.reasons.${c.getValue()}`),
        }),
        col.accessor("qty", {
          header: () => t("butcher:waste.qty"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        ...(seesCosts
          ? [
              col.accessor("cost", {
                header: () => t("butcher:waste.cost"),
                meta: { align: "end" },
                cell: (c) => <Amount value={c.getValue()} />,
              }),
            ]
          : []),
      ]),
    [t, lang, seesCosts],
  );

  return (
    <div className="flex flex-col gap-8">
      {adjusts ? (
        <section className="flex flex-col gap-4" aria-labelledby={`${id}-title`}>
          <h2 id={`${id}-title`} className="text-2xl font-bold">
            {t("butcher:waste.title")}
          </h2>
          <ProductPicker
            scanAnywhere
            onPick={(p) =>
              setDrafts((all) =>
                all.some((d) => d.productId === p.productId)
                  ? all
                  : [
                      ...all,
                      {
                        productId: p.productId,
                        name: p.name,
                        unitCode: p.unitCode,
                        qty: "",
                        reason: "damage",
                      },
                    ],
              )
            }
          />
          {drafts.length > 0 ? (
            <ul className="flex flex-col gap-2">
              {drafts.map((draft) => (
                <li
                  key={draft.productId}
                  className="flex flex-wrap items-center gap-3 rounded-card border border-line bg-surface p-3"
                >
                  <span className="min-w-40 flex-1 font-semibold">{draft.name}</span>
                  <NumericInput
                    kind="qty"
                    aria-label={t("butcher:waste.qtyOf", { name: draft.name })}
                    aria-invalid={invalid.has(draft.productId)}
                    className={cn("w-32 text-end", invalid.has(draft.productId) && "border-danger")}
                    value={draft.qty}
                    onValueChange={(qty) => patch(draft.productId, { qty })}
                  />
                  <NativeSelect
                    aria-label={t("butcher:waste.reasonOf", { name: draft.name })}
                    className="w-auto"
                    value={draft.reason}
                    onChange={(event) =>
                      patch(draft.productId, {
                        reason: REASONS.find((r) => r === event.target.value) ?? "damage",
                      })
                    }
                  >
                    {REASONS.map((reason) => (
                      <option key={reason} value={reason}>
                        {t(`butcher:waste.reasons.${reason}`)}
                      </option>
                    ))}
                  </NativeSelect>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t("butcher:waste.remove", { name: draft.name })}
                    onClick={() => setDrafts((all) => all.filter((d) => d.productId !== draft.productId))}
                  >
                    <X aria-hidden />
                  </Button>
                </li>
              ))}
            </ul>
          ) : null}
          {problem ? (
            <p role="alert" className="text-danger">
              {problem}
            </p>
          ) : null}
          {record.error ? (
            <p role="alert" className="text-danger">
              {errorText(record.error)}
            </p>
          ) : null}
          <div className="flex justify-end">
            <Button variant="primary" disabled={record.isPending} onClick={save}>
              {t("butcher:waste.save")}
            </Button>
          </div>
        </section>
      ) : null}
      <section className="flex flex-col gap-3" aria-labelledby={`${id}-day`}>
        <h3 id={`${id}-day`} className="text-xl font-semibold">
          {t("butcher:waste.day")}
        </h3>
        {day.error ? (
          <p role="alert" className="text-danger">
            {errorText(day.error)}
          </p>
        ) : null}
        <DataTable
          ariaLabel={t("butcher:waste.day")}
          data={day.data?.rows ?? []}
          columns={columns}
          getRowId={(r) => `${r.productId}:${r.reason}`}
          empty={day.isPending ? t("common:status.loading") : t("butcher:waste.empty")}
        />
        {seesCosts && day.data ? (
          <p className="font-semibold">
            {t("butcher:waste.total", { amount: formatDZD(day.data.totalCost, lang) })}
          </p>
        ) : null}
      </section>
    </div>
  );
}
