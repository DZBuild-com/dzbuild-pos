import { X } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { GuardedInput, NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { CustomerField } from "@/features/documents/editor/CustomerField";
import { toAppError } from "@/lib/ipc/client";
import { useCustomer } from "@/lib/ipc/customers";
import { useBookAppointment, useSetAppointmentStatus, useUpdateAppointment } from "@/lib/ipc/orders";
import { useProduct, useProductSearch } from "@/lib/ipc/products";
import type { Appointment, AppointmentInput, AppointmentStatus } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { atMinutes, hhmm, minutesOf, parseHhmm } from "./slots";

/** A member of the staff the book has a column for. */
export interface Staff {
  id: string;
  name: string;
}

/** Where a new appointment starts: a free slot clicked in the book. */
export interface SlotChoice {
  staffUserId: string | null;
  minutes: number;
}

const FIELDS = ["customerName", "startsAt", "durationMin", "staffUserId", "serviceProductId"] as const;
type FieldName = (typeof FIELDS)[number];
const OUTCOMES: readonly AppointmentStatus[] = ["done", "no_show", "cancelled"];

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/** A service of the catalogue (labour, a cut, a fitting), searched by name. */
function ServicePicker({ value, onChange }: { value: string | null; onChange: (id: string | null) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const id = useId();
  const [search, setSearch] = useState("");
  const query = useDebounced(normalizeDigits(search.trim()), 200);
  const hits = useProductSearch(
    { text: query, limit: 8, categoryId: null, includeArchived: false },
    value === null && query !== "",
  );
  const chosen = useProduct(value);
  const services = search.trim() === "" ? [] : (hits.data ?? []).filter((h) => h.kind === "service");
  if (value !== null) {
    return (
      <div className="flex flex-col gap-1.5">
        <span id={`${id}-label`} className="font-semibold">
          {t("orders:appointments.service")}
        </span>
        <div
          role="group"
          aria-labelledby={`${id}-label`}
          className="inline-flex min-h-11 items-center gap-1 self-start rounded-full bg-brand-soft ps-3 font-semibold"
        >
          <span>{chosen.data ? localizedName(chosen.data, lang) : "…"}</span>
          <button
            type="button"
            aria-label={t("orders:appointments.noService")}
            title={t("orders:appointments.noService")}
            onClick={() => onChange(null)}
            className="grid size-11 place-items-center rounded-full text-ink-2 hover:bg-surface focus-visible:outline-3 focus-visible:outline-focus"
          >
            <X aria-hidden className="size-4" />
          </button>
        </div>
      </div>
    );
  }
  return (
    <Field id={id} label={t("orders:appointments.service")} optional={t("orders:optional")}>
      <div className="relative">
        <Input
          id={id}
          type="search"
          dir="auto"
          autoComplete="off"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {services.length > 0 ? (
          <ul
            role="listbox"
            aria-label={t("orders:appointments.service")}
            className="absolute inset-x-0 top-full z-20 mt-1 flex flex-col rounded-control border border-line bg-surface py-1 shadow-overlay"
          >
            {services.map((hit) => (
              <li
                key={hit.id}
                role="option"
                aria-selected={false}
                onPointerDown={(e) => e.preventDefault()}
                onClick={() => {
                  setSearch("");
                  onChange(hit.id);
                }}
                className="flex min-h-11 cursor-pointer items-center px-3 hover:bg-surface-2"
              >
                {localizedName(hit, lang)}
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </Field>
  );
}

/**
 * ORD-009 lite: an appointment of the book — the customer from the file or a name and a phone, the
 * service, the staff member, the start and the duration. "Réserver" books it; an overlap the
 * server refuses (`appointment_overlap`) is shown under the field it names. An appointment booked
 * earlier can be moved, or marked done, no-show or cancelled.
 */
export function AppointmentDialog({
  day,
  staff,
  slot,
  appointment,
  onClose,
}: {
  day: string;
  staff: readonly Staff[];
  /** A new appointment in this free slot… */
  slot?: SlotChoice;
  /** …or one booked earlier. */
  appointment?: Appointment;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [customerId, setCustomerId] = useState<string | null>(appointment?.customerId ?? null);
  const [name, setName] = useState(appointment?.customerName ?? "");
  const [phone, setPhone] = useState(appointment?.phone ?? "");
  const [serviceId, setServiceId] = useState<string | null>(appointment?.serviceProductId ?? null);
  const [staffId, setStaffId] = useState<string>(appointment?.staffUserId ?? slot?.staffUserId ?? "");
  const [start, setStart] = useState(
    hhmm(appointment ? minutesOf(appointment.startsAt) : (slot?.minutes ?? 9 * 60)),
  );
  const [duration, setDuration] = useState(String(appointment?.durationMin ?? 30));
  const [note, setNote] = useState(appointment?.note ?? "");
  const [problem, setProblem] = useState<unknown>(null);
  const [invalid, setInvalid] = useState<Partial<Record<FieldName, string>>>({});
  const customer = useCustomer(customerId);
  const book = useBookAppointment();
  const update = useUpdateAppointment();
  const setStatus = useSetAppointmentStatus();
  const pending = book.isPending || update.isPending || setStatus.isPending;

  // A customer picked from the file brings his name and phone, once (both stay editable).
  const [filledFrom, setFilledFrom] = useState<string | null>(appointment?.customerId ?? null);
  const picked = customer.data;
  if (picked && picked.id !== filledFrom) {
    setFilledFrom(picked.id);
    setName(picked.name);
    setPhone(picked.phone ?? "");
  }

  const problemField = problem ? toAppError(problem).field : null;
  const fieldError = (field: FieldName) =>
    invalid[field] ?? (problemField === field ? errorText(problem) : undefined);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const minutes = parseHhmm(start);
    const length = Number(duration);
    const errors: Partial<Record<FieldName, string>> = {};
    if (name.trim() === "") errors.customerName = t("validation:required");
    if (minutes === null) errors.startsAt = t("validation:required");
    if (!Number.isInteger(length) || length < 5 || length > 600) {
      errors.durationMin = t("orders:appointments.durationRange");
    }
    setInvalid(errors);
    setProblem(null);
    if (Object.keys(errors).length > 0 || minutes === null) return;
    const input: AppointmentInput = {
      customerId,
      customerName: name.trim(),
      phone: text(phone),
      serviceProductId: serviceId,
      staffUserId: staffId === "" ? null : staffId,
      startsAt: atMinutes(day, minutes),
      durationMin: length,
      note: text(note),
    };
    const done = {
      onSuccess: () => {
        toast.success(t(appointment ? "orders:appointments.saved" : "orders:appointments.booked"));
        onClose();
      },
      onError: setProblem,
    };
    if (appointment) update.mutate({ id: appointment.id, input }, done);
    else book.mutate(input, done);
  };

  const outcome = (status: AppointmentStatus) => {
    if (!appointment) return;
    setStatus.mutate(
      { id: appointment.id, status },
      {
        onSuccess: () => {
          toast.success(t(`orders:appointments.status.${status}`));
          onClose();
        },
        onError: setProblem,
      },
    );
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t(appointment ? "orders:appointments.edit" : "orders:appointments.new")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="appointment-form" variant="primary" disabled={pending}>
            {t(appointment ? "orders:appointments.save" : "orders:appointments.book")}
          </Button>
        </>
      }
    >
      <form id="appointment-form" noValidate onSubmit={submit} className="flex flex-col gap-4">
        {appointment ? (
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={appointment.status === "booked" ? "info" : "neutral"}>
              {t(`orders:appointments.status.${appointment.status}`)}
            </Badge>
            {appointment.status === "booked"
              ? OUTCOMES.map((status) => (
                  <Button key={status} variant="outline" disabled={pending} onClick={() => outcome(status)}>
                    {t(`orders:appointments.mark.${status}`)}
                  </Button>
                ))
              : null}
          </div>
        ) : null}
        <CustomerField value={customerId} onChange={setCustomerId} />
        <div className="grid gap-3 sm:grid-cols-2">
          <Field
            id="appointment-name"
            label={t("orders:appointments.customer")}
            error={fieldError("customerName")}
          >
            <Input
              id="appointment-name"
              dir="auto"
              maxLength={120}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </Field>
          <Field
            id="appointment-phone"
            label={t("orders:appointments.phone")}
            optional={t("orders:optional")}
          >
            <GuardedInput
              id="appointment-phone"
              dir="ltr"
              inputMode="tel"
              maxLength={20}
              value={phone}
              onValueChange={setPhone}
            />
          </Field>
        </div>
        <ServicePicker value={serviceId} onChange={setServiceId} />
        <div className="grid gap-3 sm:grid-cols-3">
          <Field
            id="appointment-staff"
            label={t("orders:appointments.staff")}
            error={fieldError("staffUserId")}
          >
            <NativeSelect id="appointment-staff" value={staffId} onChange={(e) => setStaffId(e.target.value)}>
              <option value="">{t("orders:appointments.anyone")}</option>
              {staff.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </NativeSelect>
          </Field>
          <Field id="appointment-start" label={t("orders:appointments.start")} error={fieldError("startsAt")}>
            <Input
              id="appointment-start"
              type="time"
              dir="ltr"
              step={900}
              value={start}
              onChange={(e) => setStart(e.target.value)}
            />
          </Field>
          <Field
            id="appointment-duration"
            label={t("orders:appointments.duration")}
            error={fieldError("durationMin")}
          >
            <NumericInput
              id="appointment-duration"
              kind="integer"
              value={duration}
              onValueChange={setDuration}
            />
          </Field>
        </div>
        <Field id="appointment-note" label={t("orders:appointments.note")} optional={t("orders:optional")}>
          <Input
            id="appointment-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
        </Field>
        {problem && !FIELDS.some((f) => f === problemField) ? (
          <p role="alert" className="text-danger">
            {errorText(problem)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
