import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { LanguageToggle } from "@/components/language-toggle";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { CustomerField } from "@/features/documents/editor/CustomerField";
import { toAsciiDigits } from "@/lib/digits";
import type { Lang } from "@/lib/i18n";
import { useUserTiles } from "@/lib/ipc/auth";
import { useCustomer, useQuickCreateCustomer } from "@/lib/ipc/customers";
import { useIntake } from "@/lib/ipc/services";
import type { ServiceKind, ServiceTicket } from "@/lib/ipc/types";
import { normalizeDigits, parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { DepositDialog } from "./DepositDialog";
import { SERVICE_KINDS, badImei } from "./status";

interface Draft {
  kind: ServiceKind;
  item: string;
  brand: string;
  model: string;
  serial: string;
  accessories: string;
  condition: string;
  fault: string;
  estimate: string;
  promised: string;
  technicianId: string;
}

const EMPTY: Draft = {
  kind: "repair",
  item: "",
  brand: "",
  model: "",
  serial: "",
  accessories: "",
  condition: "",
  fault: "",
  estimate: "",
  promised: "",
  technicianId: "",
};

type Problems = Partial<Record<"customer" | "item" | "fault" | "serial" | "estimate", string>>;

const optional = (value: string) => (value.trim() === "" ? null : value.trim());

/**
 * ORD-005/006 "Nouveau dépôt": the item left (kind, customer — chosen or created on the spot —,
 * item, brand, model, serial or IMEI with its check, accessories, condition, fault, estimate,
 * promised day, technician) in the customer's language. Saving opens the `RP…` ticket, whose
 * intake slip prints, then offers the deposit.
 */
export function IntakeDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const intake = useIntake();
  const quick = useQuickCreateCustomer();
  const technicians = useUserTiles();
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [customerId, setCustomerId] = useState<string | null>(null);
  const customer = useCustomer(customerId);
  const [customerLang, setCustomerLang] = useState<Lang | null>(null);
  const [newCustomer, setNewCustomer] = useState<{ name: string; phone: string } | null>(null);
  const [problems, setProblems] = useState<Problems>({});
  const [saved, setSaved] = useState<ServiceTicket | null>(null);
  // the customer's file says his language; the cashier may still change it for this ticket
  const ticketLang = customerLang ?? customer.data?.lang ?? lang;
  const set = (change: Partial<Draft>) => setDraft((d) => ({ ...d, ...change }));

  const createCustomer = () => {
    if (!newCustomer || newCustomer.name.trim() === "") return;
    const phone = toAsciiDigits(newCustomer.phone.trim());
    quick.mutate(
      { name: newCustomer.name.trim(), phone: phone === "" ? null : phone },
      {
        onSuccess: (created) => {
          setCustomerId(created.id);
          setNewCustomer(null);
        },
      },
    );
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const serial = normalizeDigits(draft.serial.trim());
    const estimate = draft.estimate.trim() === "" ? null : parseAmount(draft.estimate);
    const found: Problems = {
      customer: customerId === null ? t("services:intake.customerRequired") : undefined,
      item: draft.item.trim() === "" ? t("validation:required") : undefined,
      fault: draft.fault.trim() === "" ? t("validation:required") : undefined,
      serial: badImei(serial) ? t("errors:imei_invalid") : undefined,
      estimate:
        draft.estimate.trim() !== "" && (estimate === null || estimate < 0)
          ? t("validation:amountFormat")
          : undefined,
    };
    setProblems(found);
    if (Object.values(found).some(Boolean) || customerId === null) return;
    intake.mutate(
      {
        kind: draft.kind,
        customerId,
        item: draft.item.trim(),
        brand: optional(draft.brand),
        model: optional(draft.model),
        serialNo: optional(serial),
        accessories: optional(draft.accessories),
        condition: optional(draft.condition),
        fault: draft.fault.trim(),
        estimate,
        promisedOn: optional(draft.promised),
        technicianId: optional(draft.technicianId),
        lang: ticketLang,
      },
      {
        onSuccess: (ticket) => {
          toast.success(t("services:intake.saved", { number: ticket.number }));
          setSaved(ticket);
        },
      },
    );
  };

  if (saved) {
    return (
      <DepositDialog
        ownerType="service_ticket"
        ownerId={saved.id}
        heading={t("services:deposit.for", { number: saved.number, customer: saved.customerName })}
        offered
        onClose={onClose}
      />
    );
  }

  const text = (key: keyof Draft, label: string, extra: { required?: boolean; max?: number } = {}) => (
    <Field id={`intake-${key}`} label={label} error={problems[key as keyof Problems]}>
      <Input
        id={`intake-${key}`}
        dir="auto"
        maxLength={extra.max ?? 120}
        aria-required={extra.required}
        value={draft[key]}
        onChange={(event) => set({ [key]: event.target.value })}
      />
    </Field>
  );

  return (
    <Dialog
      open
      size="lg"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("services:intake.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="service-intake" variant="primary" disabled={intake.isPending}>
            {t("services:intake.save")}
          </Button>
        </>
      }
    >
      <form id="service-intake" noValidate onSubmit={submit} className="grid gap-4 md:grid-cols-2">
        <Field id="intake-kind" label={t("services:intake.kind")}>
          <NativeSelect
            id="intake-kind"
            value={draft.kind}
            onChange={(event) =>
              set({ kind: SERVICE_KINDS.find((k) => k === event.target.value) ?? "repair" })
            }
          >
            {SERVICE_KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`services:kinds.${k}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <div className="flex flex-col gap-2">
          <CustomerField value={customerId} onChange={setCustomerId} />
          {problems.customer ? (
            <p role="alert" className="text-sm text-danger">
              {problems.customer}
            </p>
          ) : null}
          {customerId === null && newCustomer === null ? (
            <Button
              variant="ghost"
              className="self-start"
              onClick={() => setNewCustomer({ name: "", phone: "" })}
            >
              {t("services:intake.newCustomer")}
            </Button>
          ) : null}
        </div>
        {newCustomer ? (
          <fieldset className="flex flex-col gap-3 rounded-card border border-line p-3 md:col-span-2">
            <legend className="px-1 font-semibold">{t("services:intake.newCustomer")}</legend>
            <div className="grid gap-3 md:grid-cols-2">
              <Field id="intake-customer-name" label={t("services:intake.customerName")}>
                <Input
                  id="intake-customer-name"
                  dir="auto"
                  maxLength={120}
                  value={newCustomer.name}
                  onChange={(event) => setNewCustomer({ ...newCustomer, name: event.target.value })}
                />
              </Field>
              <Field id="intake-customer-phone" label={t("services:intake.customerPhone")}>
                <Input
                  id="intake-customer-phone"
                  dir="ltr"
                  type="tel"
                  maxLength={20}
                  value={newCustomer.phone}
                  onChange={(event) => setNewCustomer({ ...newCustomer, phone: event.target.value })}
                />
              </Field>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                variant="secondary"
                disabled={quick.isPending || newCustomer.name.trim() === ""}
                onClick={createCustomer}
              >
                {t("services:intake.createCustomer")}
              </Button>
              <Button variant="ghost" onClick={() => setNewCustomer(null)}>
                {t("common:actions.cancel")}
              </Button>
            </div>
            {quick.error ? (
              <p role="alert" className="text-danger">
                {errorText(quick.error)}
              </p>
            ) : null}
          </fieldset>
        ) : null}
        {text("item", t("services:intake.item"), { required: true })}
        <Field id="intake-serial" label={t("services:intake.serial")} error={problems.serial}>
          {/* a scanned IMEI is welcome here: no burst guard */}
          <Input
            id="intake-serial"
            dir="ltr"
            maxLength={40}
            autoComplete="off"
            value={draft.serial}
            onChange={(event) => set({ serial: event.target.value })}
          />
        </Field>
        {text("brand", t("services:intake.brand"), { max: 60 })}
        {text("model", t("services:intake.model"), { max: 60 })}
        {text("accessories", t("services:intake.accessories"), { max: 200 })}
        {text("condition", t("services:intake.condition"), { max: 300 })}
        <div className="md:col-span-2">
          <Field id="intake-fault" label={t("services:intake.fault")} error={problems.fault}>
            <textarea
              id="intake-fault"
              dir="auto"
              rows={2}
              maxLength={500}
              aria-required
              value={draft.fault}
              onChange={(event) => set({ fault: event.target.value })}
              className="min-h-11 w-full rounded-control border border-line bg-surface px-3 py-2 text-base text-ink"
            />
          </Field>
        </div>
        <Field id="intake-estimate" label={t("services:intake.estimate")} error={problems.estimate}>
          <NumericInput
            id="intake-estimate"
            value={draft.estimate}
            onValueChange={(estimate) => set({ estimate })}
          />
        </Field>
        <Field id="intake-promised" label={t("services:intake.promised")}>
          <Input
            id="intake-promised"
            type="date"
            dir="ltr"
            value={draft.promised}
            onChange={(event) => set({ promised: event.target.value })}
          />
        </Field>
        <Field id="intake-technician" label={t("services:intake.technician")}>
          <NativeSelect
            id="intake-technician"
            value={draft.technicianId}
            onChange={(event) => set({ technicianId: event.target.value })}
          >
            <option value="">{t("services:intake.noTechnician")}</option>
            {(technicians.data ?? []).map((u) => (
              <option key={u.id} value={u.id}>
                {u.displayName}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <div className="flex flex-col gap-1.5">
          <span className="font-semibold">{t("services:intake.lang")}</span>
          <LanguageToggle value={ticketLang} onChange={setCustomerLang} />
        </div>
        {intake.error ? (
          <p role="alert" className="text-danger md:col-span-2">
            {errorText(intake.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
