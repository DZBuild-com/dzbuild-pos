import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatDZD } from "@/lib/format";
import { useChifaOf, useEquivalents, usePharmacyProduct, useSetPharmacyProduct } from "@/lib/ipc/pharmacy";
import type { PharmacyProduct, Product } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The schedule is a free text of 20 characters at most (Tableau A/B/C). */
const SCHEDULE_MAX = 20;

/**
 * VRT-030 / PRD-051, the product form's "Médicament" tab (pharmacy module on): the DCI (with the
 * other medicines of the same DCI), the controlled flag with its schedule, "Sur ordonnance", and
 * the Chifa reference price read-only when the imported list knows the product.
 */
export function MedicineTab({ product, editable }: { product: Product; editable: boolean }) {
  const pharmacy = usePharmacyProduct(product.id);
  const errorText = useErrorText();
  if (pharmacy.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(pharmacy.error)}
      </p>
    );
  }
  if (pharmacy.isPending) return null;
  return <MedicineForm product={product} current={pharmacy.data} editable={editable} />;
}

function MedicineForm({
  product,
  current,
  editable,
}: {
  product: Product;
  current: PharmacyProduct | null;
  editable: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const id = useId();
  const [dci, setDci] = useState(current?.dci ?? "");
  const [controlled, setControlled] = useState(current?.controlled ?? false);
  const [schedule, setSchedule] = useState(current?.schedule ?? "");
  const [prescription, setPrescription] = useState(current?.prescriptionRequired ?? false);
  const save = useSetPharmacyProduct();
  const chifa = useChifaOf(product.id);
  const equivalents = useEquivalents(current?.dci ? product.id : null);

  const submit = () =>
    save.mutate(
      {
        productId: product.id,
        input: {
          dci: dci.trim() === "" ? null : dci.trim(),
          controlled,
          schedule: controlled && schedule.trim() !== "" ? schedule.trim() : null,
          prescriptionRequired: prescription,
        },
      },
      { onSuccess: () => toast.success(t("pharmacy:saved")) },
    );
  return (
    <div className="flex flex-col gap-6">
      <fieldset disabled={!editable} className="grid gap-4 sm:grid-cols-2">
        <Field id={`${id}-dci`} label={t("pharmacy:dci")}>
          <Input
            id={`${id}-dci`}
            dir="auto"
            maxLength={120}
            value={dci}
            onChange={(event) => setDci(event.target.value)}
          />
        </Field>
        <div className="flex flex-col justify-end gap-3">
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox checked={controlled} onCheckedChange={(v) => setControlled(v === true)} />
            {t("pharmacy:controlled")}
          </label>
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox checked={prescription} onCheckedChange={(v) => setPrescription(v === true)} />
            {t("pharmacy:prescription")}
          </label>
        </div>
        {controlled ? (
          <Field id={`${id}-schedule`} label={t("pharmacy:schedule")} hint={t("pharmacy:scheduleHint")}>
            <Input
              id={`${id}-schedule`}
              dir="auto"
              maxLength={SCHEDULE_MAX}
              value={schedule}
              onChange={(event) => setSchedule(event.target.value)}
            />
          </Field>
        ) : null}
      </fieldset>
      {save.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error)}
        </p>
      ) : null}
      {editable ? (
        <div className="flex justify-end">
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            {t("pharmacy:save")}
          </Button>
        </div>
      ) : null}
      <p className="rounded-control bg-surface-2 p-3">
        {chifa.data ? (
          <>
            <span className="font-semibold">
              {t("pharmacy:chifaPrice", { price: formatDZD(chifa.data.referencePrice, lang) })}
            </span>{" "}
            <Badge tone={chifa.data.reimbursable ? "ok" : "neutral"}>
              {t(chifa.data.reimbursable ? "pharmacy:reimbursable" : "pharmacy:notReimbursable")}
            </Badge>
          </>
        ) : (
          <span className="text-ink-2">{t("pharmacy:noChifa")}</span>
        )}
      </p>
      {current?.dci ? (
        <section className="flex flex-col gap-2" aria-labelledby={`${id}-same`}>
          <h3 id={`${id}-same`} className="text-lg font-semibold">
            {t("pharmacy:equivalents")}
          </h3>
          {equivalents.data && equivalents.data.length === 0 ? (
            <p className="text-ink-2">{t("pharmacy:noEquivalent")}</p>
          ) : null}
          <ul className="flex flex-col gap-2">
            {(equivalents.data ?? []).map((e) => (
              <li
                key={e.productId}
                className="flex flex-wrap items-center justify-between gap-2 rounded-control border border-line p-3"
              >
                <span className="font-semibold">{e.name}</span>
                <span className="flex items-center gap-3">
                  {e.cheaper ? <Badge tone="ok">{t("pharmacy:cheaper")}</Badge> : null}
                  <span className="text-sm text-ink-2">
                    {t("pharmacy:onHand", { qty: formatQty(e.onHand, lang) })}
                  </span>
                  <Amount value={e.price} className="font-semibold" />
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
