import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { formatDate } from "@/lib/format";
import { useAdjustStock, useStockLevels, useStockMovements } from "@/lib/ipc/stock";
import type { AdjustReason, Product, Session } from "@/lib/ipc/types";
import { formatQty, normalizeDigits, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

const REASONS: AdjustReason[] = [
  "correction",
  "opening",
  "damage",
  "loss",
  "theft",
  "expiry",
  "own_use",
  "sample",
];

/** Levels per location, the last movements (STK-015) and an adjustment with its reason (STK-007). */
export function StockPanel({ product, session }: { product: Product; session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const levels = useStockLevels(product.id);
  const movements = useStockMovements(product.id);
  const adjust = useAdjustStock();
  const [qty, setQty] = useState("");
  const [reason, setReason] = useState<AdjustReason>("correction");
  const [note, setNote] = useState("");
  const [invalid, setInvalid] = useState(false);
  const decimals = product.unitDecimals;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const text = normalizeDigits(qty.trim());
    const negative = text.startsWith("-");
    const magnitude = parseQty(negative ? text.slice(1) : text, decimals);
    if (magnitude === null) {
      setInvalid(true);
      return;
    }
    adjust.mutate(
      {
        productId: product.id,
        locationId: null,
        qtyDelta: negative ? -magnitude : magnitude,
        reason,
        unitCost: null,
        note: note.trim() === "" ? null : note.trim(),
      },
      {
        onSuccess: () => {
          setQty("");
          setNote("");
        },
      },
    );
  };
  return (
    <div className="flex flex-col gap-4">
      <ul className="flex flex-col gap-1">
        {(levels.data ?? []).map((l) => (
          <li key={l.locationId} className="flex justify-between">
            <span>
              {t(`stock:locations.${l.kind}`)} ({l.locationCode})
            </span>
            <bdi dir="ltr" className="font-bold">
              {formatQty(l.qty, lang)}
            </bdi>
          </li>
        ))}
      </ul>
      {session.permissions.includes("stock.adjust") ? (
        <form noValidate onSubmit={submit} className="grid gap-3 sm:grid-cols-3">
          <Field
            id="adj-qty"
            label={t("stock:adjust.qty")}
            error={invalid ? t("validation:qtyFormat") : undefined}
          >
            <Input
              id="adj-qty"
              dir="ltr"
              inputMode="decimal"
              value={qty}
              onChange={(e) => {
                setQty(e.target.value);
                setInvalid(false);
              }}
            />
          </Field>
          <Field id="adj-reason" label={t("stock:adjust.reason")}>
            <NativeSelect
              id="adj-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value as AdjustReason)}
            >
              {REASONS.map((r) => (
                <option key={r} value={r}>
                  {t(`stock:reasons.${r}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="adj-note" label={t("stock:adjust.note")} optional={t("setup:shop.optional")}>
            <Input id="adj-note" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
          </Field>
          <div className="sm:col-span-3">
            <Button type="submit" variant="secondary" disabled={adjust.isPending}>
              {t("stock:adjust.submit")}
            </Button>
          </div>
          {adjust.error ? (
            <p role="alert" className="text-danger sm:col-span-3">
              {errorText(adjust.error)}
            </p>
          ) : null}
        </form>
      ) : null}
      <table className="w-full border-collapse text-sm">
        <tbody>
          {(movements.data ?? []).map((m) => (
            <tr key={m.id} className="h-10 border-t border-line">
              <td>
                <bdi dir="ltr">{formatDate(m.createdAt, lang, { withTime: true })}</bdi>
              </td>
              <td>{t(`stock:movements.${m.reason}`)}</td>
              <td className="text-end">
                <bdi dir="ltr">{formatQty(m.qtyDelta, lang)}</bdi>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
