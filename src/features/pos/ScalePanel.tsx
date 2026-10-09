import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatDZD } from "@/lib/format";
import { isAppError } from "@/lib/ipc/client";
import { useClearTare, useReadStable, useTare, useWeight, useZero } from "@/lib/ipc/devices";
import { useHardwareConfig } from "@/lib/ipc/hardware";
import { usePrintWeighedLabel } from "@/lib/ipc/labels";
import { useProductGrid, useProductSearch } from "@/lib/ipc/products";
import { useKiloPrice } from "@/lib/ipc/sales";
import { labelErrorText } from "@/features/labels/warnings";
import type { LabelLang, ProductHit, ScaleState, WeightEvent } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { formatQty } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { useCart } from "./cart-store";
import type { Weighing } from "./use-customer-display";

export interface WeighedAdd {
  product: ProductHit;
  reading: WeightEvent;
}

const TONES = {
  ok: "ok",
  motion: "warn",
  underload: "warn",
  overload: "danger",
  disconnected: "danger",
  no_response: "danger",
} as const satisfies Record<ScaleState, "ok" | "warn" | "danger">;

/** DOC-034: the refusals of a label without Arabic that the cashier may confirm. */
const ARABIC_GUARDS = ["label_arabic_required", "label_arabic_name_missing"] as const;
type ArabicGuard = (typeof ARABIC_GUARDS)[number];

function arabicGuard(error: unknown): ArabicGuard | null {
  if (!isAppError(error)) return null;
  return ARABIC_GUARDS.find((code) => code === error.code) ?? null;
}

/** A reading the scale calls "ok" is only stable once the scale says so. */
function stateOf(reading: WeightEvent): ScaleState {
  return reading.state === "ok" && !reading.stable ? "motion" : reading.state;
}

/** The kilogram products the panel offers: those found by the text typed (a name, a code, a PLU),
 * else those of the category chosen on the sale screen. */
function useWeighedProducts(text: string, categoryId: string | null): ProductHit[] {
  const query = useDebounced(text.trim(), 150);
  const search = useProductSearch({ text: query, limit: 24, categoryId: null, includeArchived: false });
  const grid = useProductGrid(categoryId);
  const hits = query === "" ? grid.data : search.data;
  return (hits ?? []).filter((h) => h.unitCode === "kg" && h.kind !== "variant_parent");
}

/** F11 (VRT-010, SAL-016): the live net weight, tare and zero, the product and its price per kg;
 * "Ajouter" (F11 again) waits for a stable weight and adds a line of that weight. The panel feeds
 * the weight to the customer display through `onWeighing`. */
export function ScalePanel({
  categoryId,
  onAdd,
  onWeighing,
  onClose,
}: {
  categoryId: string | null;
  onAdd: (add: WeighedAdd) => void;
  onWeighing: (weighing: Weighing | null) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const hardware = useHardwareConfig();
  const reading = useWeight(true);
  const tare = useTare();
  const clearTare = useClearTare();
  const zero = useZero();
  const readStable = useReadStable();
  const printLabel = usePrintWeighedLabel();
  const [text, setText] = useState("");
  const [product, setProduct] = useState<ProductHit | null>(null);
  // "Imprimer sans l'arabe" asks once more before the label prints without Arabic
  const [confirming, setConfirming] = useState(false);
  const products = useWeighedProducts(text, categoryId);
  const unset = hardware.isSuccess && !hardware.data.scale;
  const state = reading ? stateOf(reading) : null;
  // The kilo as the till charges it: TTC, this customer's level and discount, today's promotion.
  const customerId = useCart((s) => s.customerId);
  const priceLevelId = useCart((s) => s.priceLevelId);
  const cartLang = useCart((s) => s.lang);
  const kilo = useKiloPrice(product?.id ?? null, { customerId, priceLevelId, lang: cartLang });
  const unitPrice = product ? (kilo.data ?? null) : null;

  // the customer screen shows the weight with the price per kg (VRT-010), until the panel closes
  useEffect(() => {
    onWeighing({ reading, unitPrice });
  }, [reading, unitPrice, onWeighing]);
  useEffect(() => () => onWeighing(null), [onWeighing]);

  // one message at a time: the last action's
  const quiet = () => {
    for (const mutation of [tare, clearTare, zero, readStable, printLabel]) mutation.reset();
    setConfirming(false);
  };
  const canAdd = product !== null && !unset && state !== "disconnected" && !readStable.isPending;
  const add = () => {
    if (!product || !canAdd) return;
    quiet();
    readStable.mutate(2_000, { onSuccess: (stable) => onAdd({ product, reading: stable }) });
  };
  // F11 again adds (the sale screen's keys are off while a dialog is open)
  const addRef = useRef(add);
  useEffect(() => {
    addRef.current = add;
  });
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "F11" || event.ctrlKey || event.altKey || event.metaKey) return;
      event.preventDefault();
      addRef.current();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  // the label carries the weight on the scale now, stable (Rust reads it again before printing)
  const print = () => {
    if (!product) return;
    quiet();
    readStable.mutate(2_000, {
      onSuccess: (stable) =>
        printLabel.mutate({
          request: { productId: product.id, net: stable.net, copies: 1, templateId: null, lang: null },
          printerId: null,
          arabicOverride: false,
        }),
    });
  };
  // the refused label again, in Arabic + French or confirmed without Arabic (journaled by Rust),
  // with the weight on the scale now: never the refused request's older weight
  const printAgain = (change: { lang?: LabelLang; arabicOverride?: boolean }) => {
    const last = printLabel.variables;
    if (!last) return;
    setConfirming(false);
    readStable.mutate(2_000, {
      onSuccess: (stable) =>
        printLabel.mutate({
          ...last,
          request: { ...last.request, net: stable.net, ...(change.lang ? { lang: change.lang } : {}) },
          arabicOverride: change.arabicOverride ?? last.arabicOverride,
        }),
    });
  };
  // the guard's buttons wait for a stable weight to print
  const steady = Boolean(reading?.stable) && !readStable.isPending && !printLabel.isPending;
  const guard = arabicGuard(printLabel.error);
  const dropped = (printLabel.data?.warnings ?? []).some((w) => w.code === "lot_or_expiry_dropped");

  const failed = [
    tare.error,
    clearTare.error,
    zero.error,
    readStable.error,
    guard ? null : printLabel.error,
  ].find(Boolean);
  const badge = unset ? (
    <Badge tone="danger">{t("hardware:scale.none")}</Badge>
  ) : state ? (
    <Badge tone={TONES[state]}>{t(`hardware:scale.states.${state}`)}</Badge>
  ) : (
    <Badge>{t("hardware:scale.waiting")}</Badge>
  );

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("pos:weigh.title")}
      footer={
        <>
          <Button
            variant="outline"
            size="lg"
            disabled={
              !product || !reading?.stable || reading.net <= 0 || printLabel.isPending || readStable.isPending
            }
            onClick={print}
          >
            {t("pos:weigh.printLabel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            className="min-w-48"
            disabled={!canAdd}
            onClick={add}
            shortcut="F11"
            shortcutLabel={t("common:keys.f11")}
          >
            {t("pos:weigh.add")}
          </Button>
        </>
      }
    >
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-card bg-surface-2 p-4">
        <bdi
          data-testid="scale-weight"
          dir="ltr"
          className="text-[3rem] leading-[1.1] font-bold tabular-nums"
        >
          {t("hardware:scale.net", { qty: formatQty(reading?.net ?? 0, lang) })}
        </bdi>
        {badge}
      </div>
      {product && unitPrice !== null ? (
        <p data-testid="kilo-price" className="font-semibold tabular-nums">
          {t("hardware:customerDisplay.perKilo", { price: formatDZD(unitPrice, lang) })}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-2">
        {[
          { key: "tare", run: () => tare.mutate(), pending: tare.isPending },
          { key: "clearTare", run: () => clearTare.mutate(), pending: clearTare.isPending },
          { key: "zero", run: () => zero.mutate(), pending: zero.isPending },
        ].map(({ key, run, pending }) => (
          <Button
            key={key}
            variant="secondary"
            disabled={unset || pending}
            onClick={() => {
              quiet();
              run();
            }}
          >
            {t(`hardware:scale.${key}`)}
          </Button>
        ))}
      </div>
      <Field id="weigh-search" label={t("pos:weigh.product")}>
        <Input
          id="weigh-search"
          dir="auto"
          autoFocus
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </Field>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
        {products.map((hit) => (
          <Button
            key={hit.id}
            variant="outline"
            aria-pressed={product?.id === hit.id}
            className={cn(
              "min-h-14 flex-col items-start gap-0.5 text-start whitespace-normal",
              product?.id === hit.id && "border-brand bg-brand-soft",
            )}
            onClick={() => {
              // a refused label belongs to the product it was asked for
              printLabel.reset();
              setConfirming(false);
              setProduct(hit);
            }}
          >
            <span>{localizedName(hit, lang)}</span>
            <span className="text-sm font-normal tabular-nums">
              {t("hardware:customerDisplay.perKilo", { price: formatDZD(hit.sellPrice, lang) })}
            </span>
          </Button>
        ))}
      </div>
      {guard ? (
        <div
          role="alert"
          className="flex flex-col gap-3 rounded-card border border-warn bg-warn-soft p-4 text-warn"
        >
          <p>
            {guard === "label_arabic_required"
              ? t("labels:print.arabicRequired", { count: 1 })
              : t("labels:print.arabicNameMissing", { count: 1 })}
          </p>
          {confirming ? (
            <div className="flex flex-wrap gap-2">
              <Button
                variant="danger"
                disabled={!steady}
                onClick={() => printAgain({ arabicOverride: true })}
              >
                {t("labels:print.confirmWithoutArabic")}
              </Button>
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                {t("common:actions.cancel")}
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap gap-2">
              {guard === "label_arabic_required" ? (
                <Button variant="secondary" disabled={!steady} onClick={() => printAgain({ lang: "ar_fr" })}>
                  {t("labels:print.switchToArabic")}
                </Button>
              ) : null}
              <Button variant="outline" onClick={() => setConfirming(true)}>
                {t("labels:print.withoutArabic")}
              </Button>
            </div>
          )}
        </div>
      ) : null}
      {dropped ? <p className="text-warn">{t("labels:print.dropped")}</p> : null}
      {failed ? (
        <p role="alert" className="text-danger">
          {labelErrorText(failed, t, errorText)}
        </p>
      ) : null}
    </Dialog>
  );
}
