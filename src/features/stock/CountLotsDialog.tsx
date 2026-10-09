import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSetCountedLot } from "@/lib/ipc/stock-m2";
import type { CountLineView, CountLotLineView } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty, parseQtyOrZero } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** One lot's counted quantity, typed: saved on blur (or Enter); an empty field changes nothing. */
function LotCountedField({
  lot,
  decimals,
  label,
  onSave,
}: {
  lot: CountLotLineView;
  decimals: number;
  label: string;
  onSave: (lotNumber: string, qty: number) => void;
}) {
  const lang = useLang();
  const initial = lot.countedQty === null ? "" : formatQty(lot.countedQty, lang);
  const [text, setText] = useState(initial);
  const [invalid, setInvalid] = useState(false);
  const commit = () => {
    if (text.trim() === "" || text === initial) return;
    const qty = parseQtyOrZero(text, decimals);
    setInvalid(qty === null);
    if (qty !== null && qty !== lot.countedQty) onSave(lot.lotNumber, qty);
  };
  return (
    <NumericInput
      kind="qty"
      aria-label={label}
      aria-invalid={invalid}
      value={text}
      onValueChange={(value) => {
        setText(value);
        setInvalid(false);
      }}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          event.currentTarget.blur();
        }
      }}
      className={cn("h-9 text-end", invalid && "border-danger")}
    />
  );
}

/**
 * STK-004: a lot-tracked product counted lot by lot. Its lots in stock at the count's
 * location are listed first expiry first, each counted on its own (`counts_set_lot`, Set mode); a
 * lot found on the shelf that the books do not know is added by its number. Once a lot is counted
 * the product's line is the sum of its lots (the backend refuses a lump sum).
 */
export function CountLotsDialog({
  countId,
  line,
  lots,
  open,
  reveal,
  onClose,
}: {
  countId: string;
  line: CountLineView;
  lots: CountLotLineView[];
  /** The count is open: the quantities can be typed. */
  open: boolean;
  /** The expected quantities are shown (not a blind count, or revealed by `stock.adjust`). */
  reveal: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const setLot = useSetCountedLot();
  const [found, setFound] = useState({ number: "", qty: "" });
  const name = localizedName(line, lang);
  const save = (lotNumber: string, qty: number) =>
    setLot
      .mutateAsync({ id: countId, productId: line.productId, lotNumber, qty, mode: "set" })
      .catch((error: unknown) => toast.error(errorText(error)));
  const foundQty = parseQtyOrZero(found.qty, line.unitDecimals);
  const addFound = () => {
    if (found.number.trim() === "" || foundQty === null || found.qty.trim() === "") return;
    void save(found.number.trim(), foundQty).then(() => setFound({ number: "", qty: "" }));
  };
  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(next) => (next ? undefined : onClose())}
      title={t("stock:counts.lots.title", { name })}
      description={t("stock:counts.lots.hint")}
      footer={
        <Button variant="outline" onClick={onClose}>
          {t("stock:counts.lots.done")}
        </Button>
      }
    >
      <table className="w-full text-start">
        <thead>
          <tr className="text-ink-2">
            <th className="p-2 text-start font-semibold">{t("stock:counts.lots.number")}</th>
            <th className="p-2 text-start font-semibold">{t("stock:counts.lots.expiry")}</th>
            {reveal ? <th className="p-2 text-end font-semibold">{t("stock:counts.expected")}</th> : null}
            <th className="p-2 text-end font-semibold">{t("stock:counts.counted")}</th>
          </tr>
        </thead>
        <tbody>
          {lots.map((lot) => (
            <tr key={lot.lotId} className="border-t border-line">
              <td className="p-2">
                <bdi dir="ltr">{lot.lotNumber}</bdi>
              </td>
              <td className="p-2">
                {lot.expiryDate ? <bdi dir="ltr">{formatDate(dayToMs(lot.expiryDate), lang)}</bdi> : null}
              </td>
              {reveal ? (
                <td className="p-2 text-end">
                  {lot.expectedQty === null ? null : <bdi dir="ltr">{formatQty(lot.expectedQty, lang)}</bdi>}
                </td>
              ) : null}
              <td className="p-2 text-end">
                {open ? (
                  <LotCountedField
                    key={`${lot.lotId}:${lot.countedQty ?? ""}`}
                    lot={lot}
                    decimals={line.unitDecimals}
                    label={t("stock:counts.lots.countedOf", { lot: lot.lotNumber })}
                    onSave={(number, qty) => void save(number, qty)}
                  />
                ) : lot.countedQty === null ? null : (
                  <bdi dir="ltr">{formatQty(lot.countedQty, lang)}</bdi>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {open ? (
        <fieldset className="mt-4 flex flex-col gap-3 rounded-card border border-line p-3">
          <legend className="px-1 font-semibold">{t("stock:counts.lots.found")}</legend>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field id="count-lot-number" label={t("stock:counts.lots.number")}>
              <Input
                id="count-lot-number"
                dir="ltr"
                maxLength={40}
                value={found.number}
                onChange={(e) => setFound({ ...found, number: e.target.value })}
              />
            </Field>
            <Field id="count-lot-qty" label={t("stock:counts.counted")}>
              <NumericInput
                id="count-lot-qty"
                kind="qty"
                value={found.qty}
                onValueChange={(qty) => setFound({ ...found, qty })}
              />
            </Field>
          </div>
          <div>
            <Button
              variant="secondary"
              disabled={setLot.isPending || found.number.trim() === "" || found.qty.trim() === ""}
              onClick={addFound}
            >
              {t("stock:counts.lots.add")}
            </Button>
          </div>
        </fieldset>
      ) : null}
    </Dialog>
  );
}
