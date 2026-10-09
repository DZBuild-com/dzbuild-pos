import { CheckCheck, Printer } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useDismissLabels, useStaleLabels } from "@/lib/ipc/labels";
import type { LabelSource, StaleLabel } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PrintLabelsDialog } from "./PrintLabelsDialog";

const col = columnHelper<StaleLabel>();

/** LBL-007: the shelf labels whose price, promotion, name or barcode changed since they were
 * printed. "Imprimer" reprints the ticked ones (or the whole queue); "Marquer à jour" drops them
 * from the queue without printing. */
export function StaleLabelsTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const stale = useStaleLabels();
  const dismiss = useDismissLabels();
  const [ticked, setTicked] = useState<ReadonlySet<string>>(new Set());
  const [printing, setPrinting] = useState<LabelSource | null>(null);
  const rows = useMemo(() => stale.data ?? [], [stale.data]);
  // what is still in the queue: a reprint or another screen may have emptied it meanwhile
  const selected = rows.filter((r) => ticked.has(r.productId)).map((r) => r.productId);
  const allTicked = rows.length > 0 && selected.length === rows.length;

  const columns = useMemo(
    () =>
      col.columns([
        col.display({
          id: "select",
          header: () => (
            <Checkbox
              aria-label={t("labels:stale.selectAll")}
              checked={allTicked}
              onCheckedChange={(on) =>
                setTicked(on === true ? new Set(rows.map((r) => r.productId)) : new Set())
              }
            />
          ),
          cell: ({ row }) => (
            <Checkbox
              aria-label={t("labels:stale.select", { name: localizedName(row.original, lang) })}
              checked={ticked.has(row.original.productId)}
              onCheckedChange={(on) =>
                setTicked((all) => {
                  const next = new Set(all);
                  if (on === true) next.add(row.original.productId);
                  else next.delete(row.original.productId);
                  return next;
                })
              }
            />
          ),
        }),
        col.accessor((r) => localizedName(r, lang), {
          id: "name",
          header: () => t("labels:stale.product"),
        }),
        col.accessor("printedPrice", {
          header: () => t("labels:stale.printed"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("price", {
          header: () => t("labels:stale.now"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} className="font-semibold" />,
        }),
        col.accessor((r) => r.oldPrice ?? -1, {
          id: "oldPrice",
          header: () => t("labels:stale.oldPrice"),
          meta: { align: "end" },
          cell: (c) => (c.row.original.oldPrice === null ? null : <Amount value={c.row.original.oldPrice} />),
        }),
        col.display({
          id: "reasons",
          header: () => t("labels:stale.reasons"),
          cell: ({ row }) => (
            <span className="flex flex-wrap gap-1">
              {row.original.reasons.map((reason) => (
                <Badge key={reason} tone={reason === "price" ? "warn" : "info"}>
                  {t(`labels:reasons.${reason}`)}
                </Badge>
              ))}
            </span>
          ),
        }),
      ]),
    [t, lang, rows, ticked, allTicked],
  );

  const markUpToDate = () =>
    dismiss.mutate(selected, {
      onSuccess: (count) => {
        setTicked(new Set());
        toast.success(t("labels:stale.dismissed", { count }));
      },
      onError: (error) => toast.error(errorText(error)),
    });

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          variant="outline"
          disabled={selected.length === 0 || dismiss.isPending}
          onClick={markUpToDate}
        >
          <CheckCheck aria-hidden />
          {t("labels:stale.dismiss")}
        </Button>
        <Button
          variant="primary"
          disabled={rows.length === 0}
          onClick={() =>
            setPrinting(
              selected.length === 0
                ? { kind: "stale" }
                : {
                    kind: "products",
                    items: selected.map((productId) => ({ productId, copies: 1, lotId: null })),
                  },
            )
          }
        >
          <Printer aria-hidden />
          {t("labels:stale.print")}
        </Button>
      </div>
      {stale.error ? (
        <p role="alert" className="text-danger">
          {errorText(stale.error)}
        </p>
      ) : null}
      <DataTable
        ariaLabel={t("labels:stale.list")}
        data={rows}
        columns={columns}
        getRowId={(r) => r.productId}
        empty={stale.isPending ? t("common:status.loading") : t("labels:stale.empty")}
      />
      {printing ? (
        <PrintLabelsDialog
          source={printing}
          onClose={() => {
            setPrinting(null);
            setTicked(new Set());
          }}
        />
      ) : null}
    </div>
  );
}
