import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { useDepartments } from "@/lib/ipc/packs";
import type { DepartmentKey } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { parseAmount, parseQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { useTillKit } from "./till-kit";

/**
 * SAL-019, VRT-001 (Ctrl+5): the department keys in place of the product grid. A key opens the
 * keypad for the price typed at the till (and a quantity), then adds an open-price line of the
 * key's product; the backend prices and taxes it like any line.
 */
export function DepartmentKeysPanel() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const { useCart } = useTillKit();
  const keys = useDepartments(false);
  const [picked, setPicked] = useState<DepartmentKey | null>(null);
  const [price, setPrice] = useState("");
  const [qty, setQty] = useState("1");
  const [invalid, setInvalid] = useState<"price" | "qty" | null>(null);

  const open = (key: DepartmentKey) => {
    setPicked(key);
    setPrice("");
    setQty("1");
    setInvalid(null);
  };

  const add = (event?: FormEvent) => {
    event?.preventDefault();
    if (!picked) return;
    const unitPrice = parseAmount(price);
    const milli = parseQty(qty.trim() === "" ? "1" : qty, 3);
    if (unitPrice === null || unitPrice <= 0) {
      setInvalid("price");
      return;
    }
    if (milli === null || milli <= 0) {
      setInvalid("qty");
      return;
    }
    useCart.getState().addLine(
      {
        productId: picked.productId,
        description: null,
        qty: milli,
        unitPrice,
        discount: { kind: "none" },
        taxRateId: null,
        note: null,
        expectedUnitPrice: null,
      },
      { name: localizedName(picked, lang), unitCode: "pc", unitDecimals: 3, price: unitPrice },
    );
    setPicked(null);
  };

  const list = keys.data ?? [];
  return (
    <section aria-label={t("packs:till.departments.title")} className="flex min-h-0 flex-1 flex-col gap-3">
      {keys.error ? (
        <p role="alert" className="text-danger">
          {errorText(keys.error)}
        </p>
      ) : null}
      {keys.isSuccess && list.length === 0 ? (
        <p className="rounded-control bg-surface p-4 text-ink-2">{t("packs:till.departments.empty")}</p>
      ) : null}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-3 overflow-y-auto">
        {list.map((key) => (
          <button
            key={key.id}
            type="button"
            onClick={() => open(key)}
            className="flex min-h-28 flex-col items-center justify-center gap-2 rounded-card border border-line bg-surface p-3 text-lg font-semibold hover:bg-surface-2 focus-visible:outline-3 focus-visible:outline-focus"
          >
            <span
              aria-hidden
              className="size-6 rounded-full border border-line"
              style={{ backgroundColor: key.color ?? "var(--surface-2)" }}
            />
            {localizedName(key, lang)}
          </button>
        ))}
      </div>
      {picked ? (
        <Dialog
          open
          onOpenChange={(isOpen) => (isOpen ? undefined : setPicked(null))}
          title={localizedName(picked, lang)}
          footer={
            <>
              <Button variant="outline" onClick={() => setPicked(null)}>
                {t("common:actions.cancel")}
              </Button>
              <Button variant="primary" onClick={() => add()}>
                {t("packs:till.departments.add")}
              </Button>
            </>
          }
        >
          <form noValidate onSubmit={add} className="grid gap-4 sm:grid-cols-2">
            <Field
              id="department-price"
              label={t("packs:till.departments.price")}
              error={invalid === "price" ? t("validation:amountFormat") : undefined}
            >
              <NumericInput
                id="department-price"
                autoFocus
                className="min-h-14 text-2xl"
                value={price}
                onValueChange={setPrice}
              />
            </Field>
            <Field
              id="department-qty"
              label={t("packs:till.departments.qty")}
              error={invalid === "qty" ? t("validation:qtyFormat") : undefined}
            >
              <NumericInput
                id="department-qty"
                kind="qty"
                className="min-h-14 text-2xl"
                value={qty}
                onValueChange={setQty}
              />
            </Field>
            {/* Enter adds the line from either field */}
            <button type="submit" hidden />
          </form>
        </Dialog>
      ) : null}
    </section>
  );
}
