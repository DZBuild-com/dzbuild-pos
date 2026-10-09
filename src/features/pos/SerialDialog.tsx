import { X } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { useSerials } from "@/lib/ipc/stock-m2";
import { useSerialsAvailable } from "@/lib/ipc/till-packs";
import { normalizeDigits } from "@/lib/numeric";
import { OWN_SCAN } from "@/lib/scanner";
import { cn } from "@/lib/utils";

/** An IMEI's check digit (Luhn, 15 digits), Task 8's rule for an instant answer at the till (the
 * backend checks again). */
export function luhnOk(digits: string): boolean {
  if (!/^\d{15}$/.test(digits)) return false;
  let sum = 0;
  for (let i = 0; i < 15; i += 1) {
    let digit = Number(digits[14 - i]);
    if (i % 2 === 1) {
      digit *= 2;
      if (digit > 9) digit -= 9;
    }
    sum += digit;
  }
  return sum % 10 === 0;
}

/** A 15-digit number is an IMEI and must pass its check; other serial numbers are free text. */
function badImei(serial: string): boolean {
  return /^\d{15}$/.test(serial) && !luhnOk(serial);
}

/**
 * SAL-025 (Ctrl+4 reopens it for the selected line): one serial number per whole piece of the
 * line, scanned or typed (a 15-digit number failing its IMEI check is refused at once) or picked
 * among the product's serials in stock. "Valider" hands the list back; the till keeps it as the
 * line's `SaleExtras.serials`.
 */
export function SerialDialog({
  productId,
  name,
  count,
  initial,
  onDone,
  onClose,
}: {
  productId: string;
  /** The product's name in the UI language. */
  name: string;
  /** Whole pieces on the line: one serial each. */
  count: number;
  initial: readonly string[];
  onDone: (serials: string[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const available = useSerialsAvailable(productId);
  const [chosen, setChosen] = useState<string[]>(() => initial.slice(0, count));
  const [text, setText] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const inStock = (available.data ?? []).map((s) => s.serialNo);
  const full = chosen.length >= count;

  const add = (typed: string) => {
    const serial = normalizeDigits(typed.trim()).toUpperCase();
    if (serial === "") return;
    const refusal = badImei(serial)
      ? t("packs:till.serial.invalid")
      : chosen.includes(serial)
        ? t("packs:till.serial.taken")
        : available.data && !inStock.includes(serial)
          ? t("packs:till.serial.notInStock")
          : null;
    setProblem(refusal);
    if (refusal || full) return;
    setChosen((all) => [...all, serial]);
    setText("");
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("packs:till.serial.title", { name })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" disabled={chosen.length !== count} onClick={() => onDone(chosen)}>
            {t("packs:till.serial.submit")}
          </Button>
        </>
      }
    >
      <form
        noValidate
        onSubmit={(event: FormEvent) => {
          event.preventDefault();
          add(text);
        }}
        className="flex items-end gap-2"
      >
        <div className="flex-1">
          <Field id="serial-field" label={t("packs:till.serial.field")} error={problem ?? undefined}>
            <ScanInput
              id="serial-field"
              {...OWN_SCAN}
              dir="ltr"
              autoFocus
              maxLength={40}
              disabled={full}
              value={text}
              onChange={(event) => setText(event.target.value)}
              onScan={add}
            />
          </Field>
        </div>
        <Button type="submit" variant="secondary" disabled={full} className={problem ? "mb-7" : undefined}>
          {t("packs:till.serial.add")}
        </Button>
      </form>
      <p role="status" className="font-semibold">
        {t("packs:till.serial.count", { done: chosen.length, total: count })}
      </p>
      {chosen.length > 0 ? (
        <ul className="flex flex-wrap gap-2">
          {chosen.map((serial) => (
            <li key={serial} className="inline-flex items-center gap-1 rounded-full bg-brand-soft ps-3">
              <bdi dir="ltr" className="tabular-nums">
                {serial}
              </bdi>
              <button
                type="button"
                aria-label={t("packs:till.serial.remove", { serial })}
                title={t("packs:till.serial.remove", { serial })}
                onClick={() => setChosen((all) => all.filter((s) => s !== serial))}
                className="grid size-11 place-items-center rounded-full text-ink-2 hover:bg-surface focus-visible:outline-3 focus-visible:outline-focus"
              >
                <X aria-hidden className="size-4" />
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {inStock.length > 0 ? (
        <section aria-label={t("packs:till.serial.inStock")} className="flex flex-col gap-2">
          <h3 className="font-semibold">{t("packs:till.serial.inStock")}</h3>
          <div className="flex flex-wrap gap-2">
            {inStock.map((serial) => (
              <Button
                key={serial}
                variant="outline"
                disabled={full || chosen.includes(serial)}
                className={cn("tabular-nums", chosen.includes(serial) && "opacity-50")}
                onClick={() => add(serial)}
              >
                <bdi dir="ltr">{serial}</bdi>
              </Button>
            ))}
          </div>
        </section>
      ) : null}
    </Dialog>
  );
}

/** SAL-025 at a return: which of the serials sold on the ticket's line come back, when only part
 * of the line does (a whole line brings all of them back by itself). */
export function ReturnSerials({
  productId,
  lineId,
  name,
  qty,
  value,
  onChange,
}: {
  productId: string;
  lineId: string;
  name: string;
  /** The quantity returned (milli-units). */
  qty: number;
  value: readonly string[];
  onChange: (serials: string[]) => void;
}) {
  const { t } = useTranslation();
  const sold = useSerials({ productId, status: "sold" });
  const ofLine = (sold.data ?? []).filter((s) => s.saleLineId === lineId).map((s) => s.serialNo);
  const pieces = Math.floor(qty / 1_000);
  if (pieces <= 0 || pieces >= ofLine.length) return null;
  return (
    <fieldset className="flex flex-col gap-2 sm:col-span-4">
      <legend className="font-semibold">{t("packs:till.serial.back", { name })}</legend>
      <p className="text-sm text-ink-2">{t("packs:till.serial.backHint", { count: pieces })}</p>
      <div className="flex flex-wrap gap-3">
        {ofLine.map((serial) => {
          const checked = value.includes(serial);
          return (
            <label key={serial} className="flex min-h-11 items-center gap-2">
              <Checkbox
                checked={checked}
                disabled={!checked && value.length >= pieces}
                onCheckedChange={(on) =>
                  onChange(on === true ? [...value, serial] : value.filter((s) => s !== serial))
                }
              />
              <bdi dir="ltr" className="tabular-nums">
                {serial}
              </bdi>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
