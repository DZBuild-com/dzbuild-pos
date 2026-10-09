import { Trash2 } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { GuardedInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useAddGuarantee } from "@/lib/ipc/instalments";
import type { GuaranteeInput, GuaranteeKind } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { GUARANTEE_KINDS } from "./kinds";

/** A supporting document as typed. */
export interface GuaranteeDraft {
  kind: GuaranteeKind;
  holder: string;
  reference: string;
  phone: string;
}

export const EMPTY_GUARANTEE: GuaranteeDraft = { kind: "id_document", holder: "", reference: "", phone: "" };

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/** True once the holder or the reference is typed: a document worth keeping. */
export const isFilled = (d: GuaranteeDraft) => d.holder.trim() !== "" || d.reference.trim() !== "";

export function toGuarantee(d: GuaranteeDraft): GuaranteeInput {
  return {
    kind: d.kind,
    holderName: text(d.holder),
    reference: text(d.reference),
    phone: text(d.phone),
    note: null,
  };
}

/** The kind (the closed list of TMP-009: never a cheque), the holder or guarantor, the document's
 * reference and a phone. */
export function GuaranteeFields({
  id,
  value,
  onChange,
}: {
  id: string;
  value: GuaranteeDraft;
  onChange: (value: GuaranteeDraft) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <Field id={`${id}-kind`} label={t("instalments:guarantees.kind")}>
        <NativeSelect
          id={`${id}-kind`}
          value={value.kind}
          onChange={(e) =>
            onChange({ ...value, kind: GUARANTEE_KINDS.find((k) => k === e.target.value) ?? "other" })
          }
        >
          {GUARANTEE_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {t(`instalments:guarantees.kinds.${kind}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <Field id={`${id}-holder`} label={t("instalments:guarantees.holder")}>
        <Input
          id={`${id}-holder`}
          dir="auto"
          maxLength={120}
          value={value.holder}
          onChange={(e) => onChange({ ...value, holder: e.target.value })}
        />
      </Field>
      <Field id={`${id}-reference`} label={t("instalments:guarantees.reference")}>
        <GuardedInput
          id={`${id}-reference`}
          dir="auto"
          maxLength={120}
          value={value.reference}
          onValueChange={(reference) => onChange({ ...value, reference })}
        />
      </Field>
      <Field id={`${id}-phone`} label={t("instalments:guarantees.phone")}>
        <GuardedInput
          id={`${id}-phone`}
          dir="ltr"
          inputMode="tel"
          maxLength={20}
          value={value.phone}
          onValueChange={(phone) => onChange({ ...value, phone })}
        />
      </Field>
    </div>
  );
}

/** The documents named on a contract (or about to be). */
export function GuaranteeList({
  items,
  onRemove,
}: {
  items: readonly GuaranteeInput[];
  onRemove?: (index: number) => void;
}) {
  const { t } = useTranslation();
  if (items.length === 0) return <p className="text-ink-2">{t("instalments:guarantees.none")}</p>;
  return (
    <ul aria-label={t("instalments:guarantees.title")} className="flex flex-col gap-2">
      {items.map((g, index) => {
        const kind = t(`instalments:guarantees.kinds.${g.kind}`);
        return (
          // the list is only appended to or cut at one place: the position is the item
          <li
            key={index}
            className="flex flex-wrap items-center gap-3 rounded-control border border-line p-3"
          >
            <span className="font-semibold">{kind}</span>
            {g.holderName ? <span>{g.holderName}</span> : null}
            {g.reference ? (
              <bdi dir="auto" className="text-ink-2">
                {g.reference}
              </bdi>
            ) : null}
            {g.phone ? (
              <bdi dir="ltr" className="text-ink-2">
                {g.phone}
              </bdi>
            ) : null}
            {onRemove ? (
              <Button
                variant="ghost"
                size="icon"
                className="ms-auto"
                aria-label={t("instalments:guarantees.remove", { kind })}
                title={t("instalments:guarantees.remove", { kind })}
                onClick={() => onRemove(index)}
              >
                <Trash2 aria-hidden />
              </Button>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

/** "Ajouter une pièce" on a signed contract (`customers.manage`). */
export function AddGuaranteeDialog({ planId, onClose }: { planId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const add = useAddGuarantee();
  const [draft, setDraft] = useState<GuaranteeDraft>(EMPTY_GUARANTEE);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    add.mutate(
      { planId, guarantee: toGuarantee(draft) },
      {
        onSuccess: () => {
          toast.success(t("instalments:guarantees.added"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("instalments:guarantees.add")}
      description={t("instalments:guarantees.noCheque")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            type="submit"
            form="guarantee-add"
            variant="primary"
            disabled={add.isPending || !isFilled(draft)}
          >
            {t("instalments:guarantees.add")}
          </Button>
        </>
      }
    >
      <form id="guarantee-add" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <GuaranteeFields id="guarantee-add" value={draft} onChange={setDraft} />
        {add.error ? (
          <p role="alert" className="text-danger">
            {errorText(add.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
