import { X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { toAppError } from "@/lib/ipc/client";
import { useSupplierReturn } from "@/lib/ipc/purchases";
import { useLots } from "@/lib/ipc/stock-m2";
import type { Supplier, SupplierReturnDone, SupplierReturnLine } from "@/lib/ipc/types";
import { parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import {
  ProductPicker,
  SerialList,
  newLineKey,
  refusedLine,
  useLineLabel,
  useTracking,
  useUnitDecimals,
} from "./purchase-lines";

/** Why goods go back to the supplier (PUR-008), written on the line. */
const REASONS = ["expired", "damaged", "wrong_item", "other"] as const;
type Reason = (typeof REASONS)[number];

interface ReturnDraft {
  key: string;
  productId: string;
  name: string;
  unitCode: string;
  qty: string;
  /** Empty: the backend takes the soonest-expiring lots first (FEFO). */
  lotNumber: string;
  serials: string[];
  reason: Reason;
}

/** The lots of a lot-tracked product in stock, soonest expiry first; none chosen = FEFO. */
function LotSelect({
  id,
  label,
  productId,
  value,
  onChange,
}: {
  id: string;
  label: string;
  productId: string;
  value: string;
  onChange: (lotNumber: string) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const lots = useLots(productId).data ?? [];
  // A lot held at several locations is listed once.
  const unique = lots.filter((l, i) => lots.findIndex((o) => o.lot.id === l.lot.id) === i);
  return (
    <Field id={id} label={t("purchases:receive.lot")}>
      <NativeSelect
        id={id}
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="">{t("purchases:return.fefo")}</option>
        {unique.map((l) => (
          <option key={l.lot.id} value={l.lot.lotNumber}>
            {l.lot.expiryDate
              ? t("purchases:return.lotWithExpiry", {
                  lot: l.lot.lotNumber,
                  date: formatDate(dayToMs(l.lot.expiryDate), lang),
                })
              : l.lot.lotNumber}
          </option>
        ))}
      </NativeSelect>
    </Field>
  );
}

/**
 * PUR-012 (spec PUR-008): expired, damaged or wrong goods back to the supplier, numbered
 * `BR…-F-…`. Lines by scan or search; a lot-tracked product leaves from the chosen lot (or the
 * soonest-expiring first), a serial-tracked one names each piece. The value (at the CMUP) is
 * credited on the supplier's account when the supplier accepts the return.
 */
export function SupplierReturnDialog({
  supplier,
  open,
  onClose,
  initial,
}: {
  supplier: Supplier;
  open: boolean;
  onClose: () => void;
  /** A line to start with: the pieces the stock page's serial register sends back. */
  initial?: { productId: string; name: string; unitCode: string; serials: string[] };
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const label = useLineLabel();
  const decimalsOf = useUnitDecimals();
  const [lines, setLines] = useState<ReturnDraft[]>(() =>
    initial
      ? [
          {
            key: newLineKey(),
            productId: initial.productId,
            name: initial.name,
            unitCode: initial.unitCode,
            qty: String(initial.serials.length),
            lotNumber: "",
            serials: initial.serials,
            reason: "damaged",
          },
        ]
      : [],
  );
  const [credit, setCredit] = useState(true);
  const [note, setNote] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [problem, setProblem] = useState<string | null>(null);
  const [sentKeys, setSentKeys] = useState<string[]>([]);
  const [done, setDone] = useState<SupplierReturnDone | null>(null);
  const tracking = useTracking(lines.map((l) => l.productId));
  const create = useSupplierReturn();
  const refusedAt = create.error ? refusedLine(toAppError(create.error).field) : null;
  const refusedKey = refusedAt !== null ? sentKeys[refusedAt] : undefined;
  const patch = (key: string, change: Partial<ReturnDraft>) => {
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...change } : l)));
    setErrors((all) => ({ ...all, [key]: "" }));
  };

  const submit = () => {
    const found: Record<string, string> = {};
    const sent: SupplierReturnLine[] = [];
    for (const line of lines) {
      const info = tracking.of(line.productId);
      const qty = parseQty(line.qty, decimalsOf(line.unitCode));
      if (qty === null) {
        found[line.key] = t("validation:qtyFormat");
      } else if (info?.serials && line.serials.length * 1_000 !== qty) {
        found[line.key] = t("purchases:receive.serialsCount", { count: line.serials.length, qty: line.qty });
      } else {
        sent.push({
          productId: line.productId,
          qty,
          unitCost: null,
          lotNumber: info?.lots && line.lotNumber !== "" ? line.lotNumber : null,
          serials: info?.serials ? line.serials : [],
          note: t(`purchases:return.reasons.${line.reason}`),
        });
      }
    }
    setErrors(found);
    setProblem(sent.length === 0 && Object.keys(found).length === 0 ? t("purchases:return.nothing") : null);
    if (sent.length === 0 || Object.keys(found).length > 0) return;
    setSentKeys(lines.map((l) => l.key));
    create.mutate(
      {
        supplierId: supplier.id,
        locationId: null,
        lines: sent,
        creditSupplier: credit,
        note: note.trim() === "" ? null : note.trim(),
      },
      {
        onSuccess: (result) => {
          toast.success(t("purchases:return.done", { number: result.document.number ?? "" }));
          setDone(result);
        },
      },
    );
  };

  if (done) {
    return (
      <Dialog
        open={open}
        onOpenChange={(o) => (o ? undefined : onClose())}
        title={t("purchases:return.doneTitle", { number: done.document.number ?? "" })}
        footer={
          <Button variant="primary" onClick={onClose}>
            {t("common:actions.done")}
          </Button>
        }
      >
        <p className="flex flex-wrap items-center gap-2">
          {t("purchases:return.value")} <Amount value={done.document.totalTtc} className="font-semibold" />
        </p>
        {done.entry ? (
          <p className="flex flex-wrap items-center gap-2">
            {t("purchases:supplier.balance")}{" "}
            <Amount value={done.entry.balanceAfter} className="font-semibold" />
          </p>
        ) : (
          <p className="text-ink-2">{t("purchases:return.notCredited")}</p>
        )}
      </Dialog>
    );
  }

  return (
    <Dialog
      open={open}
      size="xl"
      onOpenChange={(o) => (o ? undefined : onClose())}
      title={t("purchases:return.title", { name: supplier.name })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={create.isPending || !tracking.ready} onClick={submit}>
            {t("purchases:return.confirm")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <ProductPicker
          autoFocus
          scanAnywhere
          onPick={(p) =>
            setLines((all) => [
              ...all,
              {
                key: newLineKey(),
                productId: p.productId,
                name: p.name,
                unitCode: p.unitCode,
                qty: "1",
                lotNumber: "",
                serials: [],
                reason: "expired",
              },
            ])
          }
        />
        <ul className="flex flex-col gap-3">
          {lines.map((line) => {
            const info = tracking.of(line.productId);
            return (
              <li key={line.key} className="flex flex-col gap-3 rounded-card border border-line p-3">
                <div className="flex items-center justify-between gap-2">
                  <p className="font-semibold">{line.name}</p>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t("purchases:lines.removeOf", { name: line.name })}
                    onClick={() => setLines((all) => all.filter((l) => l.key !== line.key))}
                  >
                    <X aria-hidden />
                  </Button>
                </div>
                <div className="grid gap-3 sm:grid-cols-3">
                  <Field
                    id={`return-qty-${line.key}`}
                    label={t("purchases:return.qty")}
                    error={errors[line.key] || undefined}
                  >
                    <NumericInput
                      id={`return-qty-${line.key}`}
                      kind="qty"
                      aria-label={label("purchases:return.qty", line.name)}
                      aria-invalid={Boolean(errors[line.key])}
                      value={line.qty}
                      onValueChange={(qty) => patch(line.key, { qty })}
                    />
                  </Field>
                  <Field id={`return-reason-${line.key}`} label={t("purchases:return.reason")}>
                    <NativeSelect
                      id={`return-reason-${line.key}`}
                      aria-label={label("purchases:return.reason", line.name)}
                      value={line.reason}
                      onChange={(event) => {
                        const reason = REASONS.find((r) => r === event.target.value);
                        if (reason) patch(line.key, { reason });
                      }}
                    >
                      {REASONS.map((r) => (
                        <option key={r} value={r}>
                          {t(`purchases:return.reasons.${r}`)}
                        </option>
                      ))}
                    </NativeSelect>
                  </Field>
                  {info?.lots ? (
                    <LotSelect
                      id={`return-lot-${line.key}`}
                      label={label("purchases:receive.lot", line.name)}
                      productId={line.productId}
                      value={line.lotNumber}
                      onChange={(lotNumber) => patch(line.key, { lotNumber })}
                    />
                  ) : null}
                </div>
                {info?.serials ? (
                  <SerialList
                    id={`return-serials-${line.key}`}
                    label={label("purchases:receive.serials", line.name)}
                    serials={line.serials}
                    onChange={(serials) => patch(line.key, { serials })}
                  />
                ) : null}
                {refusedKey === line.key ? (
                  <p role="alert" className="text-sm text-danger">
                    {errorText(create.error)}
                  </p>
                ) : null}
              </li>
            );
          })}
        </ul>
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox checked={credit} onCheckedChange={(v) => setCredit(v === true)} />
          {t("purchases:return.credit")}
        </label>
        <Field id="return-note" label={t("purchases:receive.note")} optional={t("customers:form.optional")}>
          <Input
            id="return-note"
            dir="auto"
            maxLength={500}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {problem ? (
          <p role="alert" className="text-danger">
            {problem}
          </p>
        ) : null}
        {create.error && refusedKey === undefined ? (
          <p role="alert" className="text-danger">
            {errorText(create.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
