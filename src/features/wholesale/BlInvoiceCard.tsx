import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useTillLink } from "@/lib/ipc/lan";
import { PolicySetting } from "@/features/packs/setting-cards";
import { useModule } from "@/lib/ipc/pack-state";
import { useSetSetting, useSetting } from "@/lib/ipc/settings";
import { useErrorText } from "@/lib/use-error-text";

/** pos-core `settings::bl_as_invoice_ok`: the decision's number holds 1 to 40 characters. */
const NUMBER_MAX = 40;

interface Decision {
  number: string;
  date: string;
}

function decisionOf(value: unknown): Decision | null {
  if (typeof value !== "object" || value === null) return null;
  const { number, date } = value as Record<string, unknown>;
  return typeof number === "string" && typeof date === "string" ? { number, date } : null;
}

/** Réglages › Documents, while the wholesale module is on: the "BL valant facture" card. */
/** The wholesale minimum-order policy (final review): a line below a product's minimum order
 * warns, or is refused; shown beside the delivery-notes card while the module is on. */
export function MinOrderPolicy() {
  const { t } = useTranslation();
  const on = useModule("wholesale");
  if (!on) return null;
  return (
    <PolicySetting
      settingKey="wholesale.min_order_policy"
      title={t("packs:settings.minOrder.title")}
      help={t("packs:settings.minOrder.help")}
      warn={t("packs:settings.minOrder.warn")}
      block={t("packs:settings.minOrder.block")}
    />
  );
}

export function BlInvoiceCard() {
  const on = useModule("wholesale");
  const setting = useSetting("documents.bl_as_invoice", on);
  if (!on || setting.isPending || setting.isError) return null;
  return <BlInvoiceForm stored={decisionOf(setting.data)} />;
}

/**
 * CFG-003: the shop's delivery notes stand for invoices under a decision of the tax
 * administration. The switch reveals the decision's number and date and cannot be saved on
 * without both (the backend refuses it too); the delivery note then prints the decision's line in
 * place of "Ce bon de livraison n'est pas une facture.", shown here as it will print.
 */
function BlInvoiceForm({ stored }: { stored: Decision | null }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const till = useTillLink();
  const save = useSetSetting();
  const [enabled, setEnabled] = useState(stored !== null);
  const [number, setNumber] = useState(stored?.number ?? "");
  const [date, setDate] = useState(stored?.date ?? "");
  const [needs, setNeeds] = useState(false);
  const complete = number.trim() !== "" && date !== "";

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (enabled && !complete) {
      setNeeds(true);
      return;
    }
    save.mutate(
      {
        key: "documents.bl_as_invoice",
        value: enabled ? { number: number.trim(), date } : null,
      },
      { onSuccess: () => toast.success(t("settings:documents.saved")) },
    );
  };

  return (
    <Card id="documents-bl-invoice" className="scroll-mt-4">
      <form noValidate onSubmit={submit} className="flex flex-col gap-4">
        <h2 className="text-xl font-bold">{t("wholesale:blInvoice.title")}</h2>
        <p className="text-sm text-ink-2">{t("wholesale:blInvoice.hint")}</p>
        <label className="flex min-h-11 items-center gap-3 font-semibold">
          <Checkbox
            checked={enabled}
            onCheckedChange={(v) => {
              setEnabled(v === true);
              setNeeds(false);
            }}
          />
          {t("wholesale:blInvoice.enable")}
        </label>
        {enabled ? (
          <div className="grid gap-4 md:grid-cols-2">
            <Field id="bl-invoice-number" label={t("wholesale:blInvoice.number")}>
              <Input
                id="bl-invoice-number"
                dir="ltr"
                maxLength={NUMBER_MAX}
                aria-invalid={needs && number.trim() === ""}
                value={number}
                onChange={(e) => {
                  setNumber(e.target.value);
                  setNeeds(false);
                }}
              />
            </Field>
            <Field id="bl-invoice-date" label={t("wholesale:blInvoice.date")}>
              <Input
                id="bl-invoice-date"
                type="date"
                dir="ltr"
                aria-invalid={needs && date === ""}
                value={date}
                onChange={(e) => {
                  setDate(e.target.value);
                  setNeeds(false);
                }}
              />
            </Field>
          </div>
        ) : null}
        {needs ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {t("wholesale:blInvoice.needs")}
          </p>
        ) : null}
        <div className="flex flex-col gap-1 rounded-control bg-surface-2 p-3">
          <span className="text-sm text-ink-2">{t("wholesale:blInvoice.preview")}</span>
          <p data-testid="bl-invoice-line" className="font-semibold">
            {enabled && complete
              ? t("wholesale:blInvoice.line", { number: number.trim(), date })
              : t("wholesale:blInvoice.off")}
          </p>
        </div>
        {save.error ? (
          <p role="alert" className="rounded-control bg-danger-soft p-3 text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
        {till.isTill ? null : (
          <div className="flex justify-end">
            <Button type="submit" disabled={save.isPending}>
              {t("wholesale:blInvoice.save")}
            </Button>
          </div>
        )}
      </form>
    </Card>
  );
}
