import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { GuardedInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import type { BuyerIdentity, BuyerIdKind } from "@/lib/ipc/types";

const KINDS: BuyerIdKind[] = ["cni", "passport", "driving_licence", "other"];
const TEXTS = ["name", "idNumber", "address"] as const;

/**
 * Jewellery (VRT-141): the buyer of a precious-metal piece, written in the jeweller's special
 * register, asked before the payment when the ticket's customer file lacks the identity (NIN and
 * address): name, identity document and its number, address.
 */
export function BuyerDialog({
  initial,
  onDone,
  onClose,
}: {
  initial: BuyerIdentity | null;
  onDone: (buyer: BuyerIdentity) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [buyer, setBuyer] = useState<BuyerIdentity>(
    () => initial ?? { name: "", idKind: "cni", idNumber: "", address: "" },
  );
  const [missing, setMissing] = useState<string[]>([]);

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const empty = TEXTS.filter((key) => buyer[key].trim() === "");
    setMissing(empty);
    if (empty.length > 0) return;
    onDone({
      name: buyer.name.trim(),
      idKind: buyer.idKind,
      idNumber: buyer.idNumber.trim(),
      address: buyer.address.trim(),
    });
  };

  const text = (key: (typeof TEXTS)[number], maxLength: number) => (
    <Field
      id={`buyer-${key}`}
      label={t(`packs:till.buyer.${key}`)}
      error={missing.includes(key) ? t("packs:till.buyer.required") : undefined}
    >
      <GuardedInput
        id={`buyer-${key}`}
        maxLength={maxLength}
        value={buyer[key]}
        onValueChange={(value) => setBuyer((b) => ({ ...b, [key]: value }))}
      />
    </Field>
  );

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("packs:till.buyer.title")}
      description={t("packs:till.buyer.hint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={() => submit()}>
            {t("packs:till.buyer.submit")}
          </Button>
        </>
      }
    >
      <form noValidate onSubmit={submit} className="grid gap-3 sm:grid-cols-2">
        <div className="sm:col-span-2">{text("name", 120)}</div>
        <Field id="buyer-idKind" label={t("packs:till.buyer.idKind")}>
          <NativeSelect
            id="buyer-idKind"
            value={buyer.idKind}
            onChange={(e) => setBuyer((b) => ({ ...b, idKind: e.target.value as BuyerIdKind }))}
          >
            {KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {t(`packs:till.buyer.idKinds.${kind}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {text("idNumber", 40)}
        <div className="sm:col-span-2">{text("address", 200)}</div>
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
