import { type FormEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { LANGS, type Lang, i18n, loadLanguage } from "@/lib/i18n";
import { useUnits } from "@/lib/ipc/catalog";
import { toAppError } from "@/lib/ipc/client";
import type { Product, Unit } from "@/lib/ipc/types";
import { useCreatePack } from "@/lib/ipc/wholesale";
import { localizedName } from "@/lib/names";
import { normalizeDigits, parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The packagings a pack can come in, by their seeded unit codes (migrations 0002 and 0008). */
const PACK_UNITS = ["carton", "bag", "pallet", "box", "lot"] as const;
type PackUnit = (typeof PACK_UNITS)[number];

const FIELD: Record<Lang, "nameAr" | "nameFr" | "nameEn"> = { ar: "nameAr", fr: "nameFr", en: "nameEn" };

/** Every language's texts in memory, so the pack's three names can be written in their own words. */
function useAllLanguages(): boolean {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    let live = true;
    void Promise.all(LANGS.map((lang) => loadLanguage(lang))).then(() => {
      if (live) setReady(true);
    });
    return () => {
      live = false;
    };
  }, []);
  return ready;
}

/** "Eau minérale 1,5 L — carton de 12" in each language: the unit product's name in that language
 * (else its French one), the packaging's unit name and the count. */
function packNames(product: Product, unit: Unit, count: number): Record<Lang, string> {
  const names = {} as Record<Lang, string>;
  for (const lang of LANGS) {
    const name = product[FIELD[lang]].trim() || product.nameFr;
    const unitName = (unit[FIELD[lang]].trim() || unit.nameFr).toLocaleLowerCase(lang);
    names[lang] = i18n.getFixedT(lang, "wholesale")("pack.defaultName", { name, unit: unitName, n: count });
  }
  return names;
}

/**
 * PRD-033, "Créer un pack" on a simple product: a carton, bag, pallet, box or lot of N units sold
 * as a product of its own — its price and barcode its own, its stock the unit's (each pack sold
 * takes N units). The names start as "… — carton de 12" in the three languages and follow the
 * count and packaging until they are typed over.
 */
export function PackDialog({ product, onClose }: { product: Product; onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const units = useUnits().data ?? [];
  const create = useCreatePack();
  const languages = useAllLanguages();
  const [count, setCount] = useState("");
  const [packUnit, setPackUnit] = useState<PackUnit>("carton");
  const [typed, setTyped] = useState<Partial<Record<Lang, string>>>({});
  const [price, setPrice] = useState("");
  const [barcode, setBarcode] = useState("");
  const [invalid, setInvalid] = useState<"count" | "price" | null>(null);
  const offered = PACK_UNITS.flatMap((code) => {
    const unit = units.find((u) => u.code === code && !u.archived);
    return unit ? [{ code, unit }] : [];
  });
  const unit = offered.find((o) => o.code === packUnit)?.unit;
  const pieces = Number(normalizeDigits(count));
  const whole = /^\d+$/.test(normalizeDigits(count)) && pieces >= 2 ? pieces : null;
  const defaults = languages && unit && whole !== null ? packNames(product, unit, whole) : null;
  const nameOf = (l: Lang) => typed[l] ?? defaults?.[l] ?? "";
  const refused = toAppError(create.error);
  const fieldError = (field: string) =>
    create.error && refused.field === field ? errorText(create.error) : undefined;
  const placed = ["unitsPerPack", "price", "barcode", "nameAr", "nameFr", "nameEn"].includes(
    refused.field ?? "",
  );

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const amount = parseAmount(price);
    if (whole === null) {
      setInvalid("count");
      return;
    }
    if (amount === null || amount <= 0) {
      setInvalid("price");
      return;
    }
    if (!unit) return;
    create.mutate(
      {
        unitProductId: product.id,
        unitsPerPack: whole * 1_000,
        unitId: unit.id,
        nameAr: nameOf("ar").trim(),
        nameFr: nameOf("fr").trim(),
        nameEn: nameOf("en").trim(),
        price: amount,
        barcode: barcode.trim() === "" ? null : normalizeDigits(barcode.trim()),
      },
      {
        onSuccess: (pack) => {
          toast.success(t("wholesale:pack.created", { name: localizedName(pack, lang) }));
          onClose();
        },
      },
    );
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:pack.create")}
      description={t("wholesale:pack.hint", { product: localizedName(product, lang) })}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="pack-form" variant="primary" disabled={create.isPending}>
            {t("wholesale:pack.submit")}
          </Button>
        </>
      }
    >
      <form id="pack-form" noValidate onSubmit={submit} className="grid gap-4 sm:grid-cols-2">
        <Field
          id="pack-units"
          label={t("wholesale:pack.units")}
          hint={t("wholesale:pack.unitsHint")}
          error={invalid === "count" ? t("wholesale:pack.unitsHint") : fieldError("unitsPerPack")}
        >
          <NumericInput
            id="pack-units"
            kind="integer"
            autoFocus
            maxLength={6}
            aria-invalid={invalid === "count"}
            value={count}
            onValueChange={(v) => {
              setCount(v);
              setInvalid(null);
            }}
          />
        </Field>
        <Field id="pack-unit" label={t("wholesale:pack.unit")}>
          <NativeSelect
            id="pack-unit"
            value={packUnit}
            onChange={(event) => setPackUnit(PACK_UNITS.find((c) => c === event.target.value) ?? "carton")}
          >
            {offered.map((o) => (
              <option key={o.code} value={o.code}>
                {t(`wholesale:pack.units_names.${o.code}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {LANGS.map((l) => {
          const key = FIELD[l];
          return (
            <Field key={l} id={`pack-${key}`} label={t(`products:fields.${key}`)} error={fieldError(key)}>
              <Input
                id={`pack-${key}`}
                lang={l}
                dir={l === "ar" ? "rtl" : "ltr"}
                maxLength={120}
                value={nameOf(l)}
                onChange={(event) => setTyped((all) => ({ ...all, [l]: event.target.value }))}
              />
            </Field>
          );
        })}
        <Field
          id="pack-price"
          label={t("wholesale:pack.price")}
          error={invalid === "price" ? t("validation:amountFormat") : fieldError("price")}
        >
          <NumericInput
            id="pack-price"
            aria-invalid={invalid === "price"}
            value={price}
            onValueChange={(v) => {
              setPrice(v);
              setInvalid(null);
            }}
          />
        </Field>
        <Field
          id="pack-barcode"
          label={t("wholesale:pack.barcode")}
          optional={t("wholesale:rounds.optional")}
          error={fieldError("barcode")}
        >
          <ScanInput
            id="pack-barcode"
            dir="ltr"
            maxLength={48}
            value={barcode}
            onChange={(event) => setBarcode(event.target.value)}
            onScan={setBarcode}
          />
        </Field>
        {create.error && !placed ? (
          <p role="alert" className="text-danger sm:col-span-2">
            {errorText(create.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
