import { ArrowLeft, Plus, TriangleAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { ExportMenu } from "@/components/data-table/export-menu";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { NativeSelect } from "@/components/ui/native-select";
import { useCan } from "@/features/stock/stock-parts";
import { dayToMs, today } from "@/lib/dates";
import { formatDZD, formatDate } from "@/lib/format";
import { useArchivePromotion, usePromotions } from "@/lib/ipc/promotions";
import type { Campaign, ExportTarget, Promotion } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, formatRate } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { PromotionForm } from "./PromotionForm";

const CAMPAIGNS: readonly Campaign[] = ["promotion", "solde", "liquidation", "deballage"];
const col = columnHelper<Promotion>();

/**
 * `/products/promotions` (PRM-001…006, 021, 022): every promotion and campaign with its type,
 * dates, how it applies and whether it is running; soldes the backend finds outside the wali's
 * windows are flagged. A row opens the promotion (editable with `products.price`); a legal
 * campaign exports its declaration (the report `promotion_declaration`, with `reports.sales`).
 */
export function PromotionsPage() {
  const { t } = useTranslation();
  const lang = useLang();
  const can = useCan();
  const [search, setSearch] = useState("");
  const [campaign, setCampaign] = useState<Campaign | "">("");
  const [showArchived, setShowArchived] = useState(false);
  const [open, setOpen] = useState<Promotion | "new" | null>(null);
  const text = useDebounced(search.trim().toLocaleLowerCase(), 150);
  const list = usePromotions(showArchived);
  const rows = useMemo(
    () =>
      (list.data ?? []).filter(
        (p) =>
          (campaign === "" || p.campaign === campaign) &&
          (text === "" || [p.nameFr, p.nameAr, p.nameEn].some((n) => n.toLocaleLowerCase().includes(text))),
      ),
    [list.data, campaign, text],
  );
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor((p) => localizedName(p, lang), {
          id: "name",
          header: () => t("promotions:columns.name"),
          cell: (c) => <span className="font-semibold">{c.getValue()}</span>,
        }),
        col.accessor("campaign", {
          header: () => t("promotions:columns.campaign"),
          cell: (c) => (
            <Badge tone={c.getValue() === "promotion" ? "neutral" : "info"}>
              {t(`promotions:campaigns.${c.getValue()}`)}
            </Badge>
          ),
        }),
        col.display({
          id: "discount",
          header: () => t("promotions:columns.discount"),
          cell: ({ row }) => <Discount promotion={row.original} />,
        }),
        col.accessor((p) => p.startsOn ?? "", {
          id: "period",
          header: () => t("promotions:columns.period"),
          cell: ({ row }) => {
            const { startsOn, endsOn } = row.original;
            if (startsOn && endsOn)
              return (
                <span>
                  <Day day={startsOn} /> – <Day day={endsOn} />
                </span>
              );
            if (startsOn) return t("promotions:period.from", { from: formatDate(dayToMs(startsOn), lang) });
            if (endsOn) return t("promotions:period.to", { to: formatDate(dayToMs(endsOn), lang) });
            return <span className="text-ink-2">{t("promotions:period.none")}</span>;
          },
        }),
        col.accessor("triggerMode", {
          header: () => t("promotions:columns.trigger"),
          cell: (c) => t(`promotions:triggerTags.${c.getValue()}`),
        }),
        col.display({
          id: "status",
          header: () => t("promotions:columns.status"),
          cell: ({ row }) => {
            const p = row.original;
            return (
              <span className="flex flex-wrap items-center gap-1">
                {p.archived ? (
                  <Badge>{t("promotions:status.archived")}</Badge>
                ) : (
                  <Badge tone={p.active ? "ok" : "neutral"}>
                    {t(p.active ? "promotions:status.active" : "promotions:status.inactive")}
                  </Badge>
                )}
                {p.outsideSoldesWindow ? (
                  <Badge tone="danger" className="gap-1">
                    <TriangleAlert aria-hidden className="size-4" />
                    {t("promotions:status.outside")}
                  </Badge>
                ) : null}
              </span>
            );
          },
        }),
      ]),
    [t, lang],
  );

  return (
    <div className="flex flex-col gap-4">
      <Link
        to="/products"
        className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
      >
        <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
        {t("promotions:back")}
      </Link>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("promotions:title")}</h1>
        {can("products.price") ? (
          <Button variant="primary" onClick={() => setOpen("new")}>
            <Plus aria-hidden />
            {t("promotions:new")}
          </Button>
        ) : null}
      </div>
      <TableToolbar
        search={search}
        onSearch={setSearch}
        // No export of this list exists; a legal campaign exports its declaration.
        exportTarget={null}
        filters={
          <div className="flex flex-wrap items-end gap-3">
            <NativeSelect
              aria-label={t("promotions:columns.campaign")}
              className="w-auto"
              value={campaign}
              onChange={(e) => setCampaign(e.target.value as Campaign | "")}
            >
              <option value="">{t("promotions:filters.allCampaigns")}</option>
              {CAMPAIGNS.map((c) => (
                <option key={c} value={c}>
                  {t(`promotions:campaigns.${c}`)}
                </option>
              ))}
            </NativeSelect>
            <label className="flex min-h-11 items-center gap-3 font-semibold">
              <Checkbox checked={showArchived} onCheckedChange={(v) => setShowArchived(v === true)} />
              {t("promotions:filters.showArchived")}
            </label>
          </div>
        }
      />
      <DataTable
        ariaLabel={t("promotions:title")}
        data={rows}
        columns={columns}
        getRowId={(p) => p.id}
        onRowClick={setOpen}
        empty={list.isPending ? t("common:status.loading") : t("promotions:empty")}
      />
      {open !== null ? (
        <PromotionDialog
          key={open === "new" ? "new" : open.id}
          promotion={open === "new" ? null : open}
          onClose={() => setOpen(null)}
        />
      ) : null}
    </div>
  );
}

function Day({ day }: { day: string }) {
  const lang = useLang();
  return <bdi dir="ltr">{formatDate(dayToMs(day), lang)}</bdi>;
}

/** "30 %", "50,00 DA", "2 achetés + 1 offerts", "3 pour 500,00 DA". */
function Discount({ promotion: p }: { promotion: Promotion }) {
  const { t } = useTranslation();
  const lang = useLang();
  switch (p.kind) {
    case "percent":
      return <bdi dir="ltr">{formatRate(p.value, lang)}</bdi>;
    case "amount":
      return <Amount value={p.value} />;
    case "buy_x_get_y":
      return (
        <span>
          {t("promotions:describe.buyGet", {
            buy: formatQty(p.buyQty ?? 0, lang),
            get: formatQty(p.getQty ?? 0, lang),
          })}
        </span>
      );
    case "bundle_price":
      return (
        <span>
          {t("promotions:describe.bundle", {
            qty: formatQty(p.buyQty ?? 0, lang),
            price: formatDZD(p.value, lang),
          })}
        </span>
      );
  }
}

function PromotionDialog({ promotion, onClose }: { promotion: Promotion | null; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const can = useCan();
  const archive = useArchivePromotion();
  const edits = can("products.price") && !promotion?.archived;
  const legal = promotion !== null && promotion.campaign !== "promotion";
  const declaration: ExportTarget | null =
    legal && can("reports.sales")
      ? {
          kind: "report",
          request: {
            kind: "promotion_declaration",
            from: promotion.startsOn ?? today(),
            to: promotion.endsOn ?? today(),
            promotionId: promotion.id,
          },
        }
      : null;
  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={promotion ? localizedName(promotion, lang) : t("promotions:new")}
    >
      <PromotionForm
        promotion={promotion}
        readOnly={!edits}
        onCancel={onClose}
        onSaved={() => {
          toast.success(t("promotions:saved"));
          onClose();
        }}
      />
      {declaration ? (
        <section className="flex flex-col gap-2 border-t border-line pt-4">
          <h3 className="text-lg font-bold">{t("promotions:declaration.title")}</h3>
          <p className="text-ink-2">{t("promotions:declaration.body")}</p>
          <ExportMenu target={declaration} />
        </section>
      ) : null}
      {promotion && edits ? (
        <div className="flex justify-start border-t border-line pt-4">
          <Button
            variant="danger"
            disabled={archive.isPending}
            onClick={() =>
              archive.mutate(promotion.id, {
                onSuccess: () => {
                  toast.success(t("promotions:archived"));
                  onClose();
                },
              })
            }
          >
            {t("promotions:archive")}
          </Button>
        </div>
      ) : null}
      {archive.error ? (
        <p role="alert" className="text-danger">
          {errorText(archive.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
