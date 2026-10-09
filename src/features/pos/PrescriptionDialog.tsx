import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { GuardedInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { PrescriptionInput } from "@/lib/ipc/types";

interface Form {
  prescriberName: string;
  prescriberRef: string;
  prescriptionNo: string;
  prescriptionDate: string;
  patientName: string;
  patientRef: string;
}

const EMPTY: Form = {
  prescriberName: "",
  prescriberRef: "",
  prescriptionNo: "",
  prescriptionDate: "",
  patientName: "",
  patientRef: "",
};

const REQUIRED = ["prescriberName", "prescriptionDate", "patientName"] as const;

const optional = (value: string): string | null => (value.trim() === "" ? null : value.trim());

/**
 * Pharmacy (Task 6): the controlled medicines register's entry of each controlled line of the
 * ticket, asked before the payment — prescriber and reference, prescription number and date,
 * patient and reference. The data lives only in the sale's request and the register (written on
 * the main PC); the sale needs `pharmacy.register` or a manager's approval, asked at "Valider".
 */
export function PrescriptionDialog({
  lines,
  initial,
  onDone,
  onClose,
}: {
  /** The controlled lines: their index in the cart and the medicine's name. */
  lines: { line: number; name: string }[];
  initial: readonly PrescriptionInput[];
  onDone: (prescriptions: PrescriptionInput[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [forms, setForms] = useState<Form[]>(() =>
    lines.map(({ line }) => {
      const known = initial.find((p) => p.line === line);
      return known
        ? {
            prescriberName: known.prescriberName,
            prescriberRef: known.prescriberRef ?? "",
            prescriptionNo: known.prescriptionNo ?? "",
            prescriptionDate: known.prescriptionDate,
            patientName: known.patientName,
            patientRef: known.patientRef ?? "",
          }
        : EMPTY;
    }),
  );
  const [missing, setMissing] = useState<Set<string>>(new Set());
  const patch = (index: number, change: Partial<Form>) =>
    setForms((all) => all.map((f, i) => (i === index ? { ...f, ...change } : f)));

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const empty = new Set<string>();
    forms.forEach((form, i) => {
      for (const field of REQUIRED) if (form[field].trim() === "") empty.add(`${i}-${field}`);
    });
    setMissing(empty);
    if (empty.size > 0) return;
    onDone(
      forms.map((form, i) => ({
        line: lines[i]?.line ?? 0,
        prescriberName: form.prescriberName.trim(),
        prescriberRef: optional(form.prescriberRef),
        prescriptionNo: optional(form.prescriptionNo),
        prescriptionDate: form.prescriptionDate,
        patientName: form.patientName.trim(),
        patientRef: optional(form.patientRef),
      })),
    );
  };

  const field = (i: number, key: keyof Form, label: string, maxLength = 120) => {
    const id = `rx-${i}-${key}`;
    return (
      <Field
        id={id}
        label={t(`packs:till.prescription.${label}`)}
        error={missing.has(`${i}-${key}`) ? t("packs:till.prescription.required") : undefined}
      >
        {key === "prescriptionDate" ? (
          <Input
            id={id}
            type="date"
            dir="ltr"
            value={forms[i]?.[key] ?? ""}
            onChange={(e) => patch(i, { [key]: e.target.value })}
          />
        ) : (
          <GuardedInput
            id={id}
            maxLength={maxLength}
            value={forms[i]?.[key] ?? ""}
            onValueChange={(value) => patch(i, { [key]: value })}
          />
        )}
      </Field>
    );
  };

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("packs:till.prescription.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={() => submit()}>
            {t("packs:till.prescription.submit")}
          </Button>
        </>
      }
    >
      <form noValidate onSubmit={submit} className="flex flex-col gap-6">
        {lines.map(({ line, name }, i) => (
          <section key={line} aria-label={name} className="flex flex-col gap-3">
            {lines.length > 1 ? <h3 className="font-bold">{name}</h3> : null}
            <div className="grid gap-3 sm:grid-cols-2">
              {field(i, "prescriberName", "prescriber")}
              {field(i, "prescriberRef", "prescriberRef", 40)}
              {field(i, "prescriptionNo", "number", 40)}
              {field(i, "prescriptionDate", "date")}
              {field(i, "patientName", "patient")}
              {field(i, "patientRef", "patientRef", 40)}
            </div>
          </section>
        ))}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
