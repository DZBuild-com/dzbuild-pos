import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { approvalPermission } from "@/lib/approval";
import { formatDZD } from "@/lib/format";
import type { PaymentPreview } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { type DraftLine } from "./cart-store";

/** End column (spec §10.5): lines, totals, the amount-due band, the pay button and the POS
 * buttons. Every figure comes from the backend's preview; before it answers, a line shows "…". */
export function CartPanel({
  lines,
  selected,
  preview,
  error,
  onSelect,
  onEdit,
  onApproval,
  payButton,
  actions,
  banner,
  customer,
  notices,
}: {
  lines: DraftLine[];
  selected: number | null;
  preview: PaymentPreview | undefined;
  error: unknown;
  onSelect: (index: number) => void;
  onEdit: (index: number) => void;
  onApproval: () => void;
  payButton?: ReactNode;
  actions: ReactNode;
  banner?: ReactNode;
  /** The ticket's customer (F3), under the title. */
  customer?: ReactNode;
  /** M8 pharmacy: what is said under a product's line (a cheaper generic, the Chifa price). */
  notices?: (index: number, productId: string) => ReactNode;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const totals = preview?.cart.totals;
  const ht = preview?.cart.priceMode === "ht";
  const settlement = preview?.settlement;
  return (
    <aside className="flex w-[440px] shrink-0 flex-col border-s border-line bg-surface max-[1100px]:w-[400px]">
      <div className="flex items-center justify-between border-b border-line px-4 py-3">
        <h2 className="text-xl font-bold">{t("pos:cart.title")}</h2>
        {banner}
      </div>
      {customer}
      <ul aria-label={t("pos:cart.title")} className="min-h-0 flex-1 overflow-y-auto">
        {lines.length === 0 ? <li className="p-6 text-ink-2">{t("pos:cart.empty")}</li> : null}
        {lines.map((draft, index) => {
          const priced = preview?.cart.lines[index];
          return (
            <li key={draft.key}>
              <button
                type="button"
                aria-current={index === selected ? "true" : undefined}
                // Spec §10.5 "tap to edit": a first tap selects the line, a tap on the selected
                // line opens it (double-click still does, for the mouse)
                onClick={() => (index === selected ? onEdit(index) : onSelect(index))}
                onDoubleClick={() => onEdit(index)}
                className={cn(
                  "flex w-full items-start justify-between gap-3 border-b border-line px-4 py-3 text-start",
                  index === selected ? "bg-brand-soft" : "hover:bg-surface-2",
                )}
              >
                <span className="flex flex-col">
                  <span className="font-semibold">
                    {t("pos:cart.line", {
                      qty: formatQty(draft.line.qty, lang),
                      name: priced?.description ?? draft.display.name,
                    })}
                  </span>
                  {priced && priced.amounts.discountAmount !== 0 ? (
                    <span className="text-sm text-ok">
                      {/* PRM-005/006, PRC-003: an automatic discount says where it comes from */}
                      <span>
                        {priced.autoDiscount
                          ? (priced.autoDiscount.label ?? t("pos:cart.customerDiscount"))
                          : t("pos:cart.discount")}
                      </span>{" "}
                      <Amount value={-priced.amounts.discountAmount} />
                    </span>
                  ) : null}
                  {priced?.priceChanged ? (
                    <span className="text-sm text-warn">{t("pos:cart.priceChanged")}</span>
                  ) : null}
                  {/* M8: a regulated staple's ceiling (SAL-030), the customer's agreed price, a
                      wholesale minimum not reached */}
                  {priced?.regulated ? (
                    <span
                      className={cn(
                        "w-fit rounded-sm px-1 text-sm",
                        priced.regulated.above ? "bg-danger-soft text-danger" : "text-ink-2",
                      )}
                    >
                      {t("pos:cart.regulated", { ceiling: formatDZD(priced.regulated.ceiling, lang) })}{" "}
                      {priced.regulated.above ? <span>{t("pos:cart.above")}</span> : null}
                    </span>
                  ) : null}
                  {priced?.customerPrice ? (
                    <span className="text-sm text-info">{t("pos:cart.customerPrice")}</span>
                  ) : null}
                  {priced?.minOrder ? (
                    <span className="w-fit rounded-sm bg-warn-soft px-1 text-sm text-warn">
                      {t("pos:cart.minOrder", { qty: formatQty(priced.minOrder, lang) })}
                    </span>
                  ) : null}
                </span>
                <span className="text-2xl font-bold">
                  {priced ? (
                    <Amount value={priced.amounts.totalTtc} />
                  ) : (
                    <span aria-label={t("pos:cart.pricing")}>…</span>
                  )}
                </span>
              </button>
              {notices && draft.line.productId ? notices(index, draft.line.productId) : null}
            </li>
          );
        })}
      </ul>
      {error ? (
        <div
          role="alert"
          className="flex flex-col gap-2 border-t border-danger bg-danger-soft px-4 py-3 text-danger"
        >
          <p>{errorText(error)}</p>
          {approvalPermission(error) ? (
            <Button variant="danger" onClick={onApproval}>
              {t("pos:cart.askApproval")}
            </Button>
          ) : null}
        </div>
      ) : null}
      {/* Spec §10.5: subtotal (before discount, in the price mode: TTC, or HT with the TVA added
          under it), discount, TVA under the réel regime, stamp duty. All backend figures. */}
      <dl
        role="group"
        aria-label={t("pos:cart.totals")}
        className="grid gap-1 border-t border-line px-4 py-3"
      >
        <div className="flex justify-between">
          <dt>{t("pos:cart.items")}</dt>
          <dd>
            <bdi dir="ltr">{preview ? formatQty(preview.cart.itemCount, lang) : "0"}</bdi>
          </dd>
        </div>
        {totals ? (
          <div className="flex justify-between">
            <dt>{t(ht ? "pos:cart.subtotalHt" : "pos:cart.subtotal")}</dt>
            <dd>
              <Amount value={totals.grossTotal} />
            </dd>
          </div>
        ) : null}
        {totals && totals.discountTotal !== 0 ? (
          <div className="flex justify-between text-ok">
            <dt>{t("pos:cart.discount")}</dt>
            <dd>
              <Amount value={-totals.discountTotal} />
            </dd>
          </div>
        ) : null}
        {totals && preview?.cart.taxRegime === "reel" ? (
          <div className="flex justify-between">
            <dt>{t(ht ? "pos:cart.vatAdded" : "pos:cart.vat")}</dt>
            <dd>
              <Amount value={totals.taxTotal} />
            </dd>
          </div>
        ) : null}
        {settlement && settlement.stampCharged !== 0 ? (
          <div className="flex justify-between">
            <dt>{t("pos:cart.stampDuty")}</dt>
            <dd>
              <Amount value={settlement.stampCharged} />
            </dd>
          </div>
        ) : null}
      </dl>
      <div
        role="status"
        aria-label={t("pos:cart.due")}
        className="flex items-baseline justify-between bg-brand px-4 py-3 text-brand-ink"
      >
        <span className="text-lg font-semibold">{t("pos:cart.due")}</span>
        <Amount value={settlement?.totalDue ?? 0} className="text-5xl leading-none font-bold" />
      </div>
      <div className="flex flex-col gap-2 p-3">
        {payButton}
        <div className="grid grid-cols-3 gap-2">{actions}</div>
      </div>
    </aside>
  );
}
