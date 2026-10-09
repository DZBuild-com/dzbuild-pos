import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useUserTiles } from "@/lib/ipc/auth";
import { useAppointmentsDay } from "@/lib/ipc/orders";
import { useProduct } from "@/lib/ipc/products";
import type { Appointment } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { AppointmentDialog, type SlotChoice, type Staff } from "./AppointmentDialog";
import { OPENS, SLOTS, SLOT_MINUTES, hhmm, minutesOf, slotOf } from "./slots";

/** The column of the appointments booked with nobody in particular. */
const ANYONE = "";

/** The service's name in the UI language (the backend's is the French one). */
function ServiceName({ appointment }: { appointment: Appointment }) {
  const lang = useLang();
  const product = useProduct(appointment.serviceProductId);
  const name = product.data ? localizedName(product.data, lang) : appointment.serviceName;
  return name ? <span className="truncate">{name}</span> : null;
}

type Open = { slot: SlotChoice } | { appointment: Appointment } | null;

/** Where an appointment sits: its staff column and its rows (an appointment past 20:00 is cut). */
interface Placed {
  appointment: Appointment;
  column: number;
  row: number;
  span: number;
}

/**
 * ORD-009 lite: one day of appointments, a column per member of the staff (and one for "anyone"
 * when an appointment has no one), 15-minute rows from 08:00 to 20:00. An appointment is a block
 * over its rows; every free slot is a button ("Nadia 10:00 libre") that books it. The grid follows
 * the language's direction: the first column is at the start side.
 */
export function AppointmentBook({ day }: { day: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const users = useUserTiles();
  const appointments = useAppointmentsDay(day);
  const [open, setOpen] = useState<Open>(null);

  const shown = (appointments.data ?? []).filter((a) => a.status !== "cancelled");
  const staff: Staff[] = (users.data ?? []).map((u) => ({ id: u.id, name: u.displayName }));
  const known = new Set(staff.map((s) => s.id));
  const columns: Staff[] = shown.some((a) => !a.staffUserId || !known.has(a.staffUserId))
    ? [...staff, { id: ANYONE, name: t("orders:appointments.anyone") }]
    : staff;
  const columnOf = (a: Appointment) =>
    columns.findIndex((c) => c.id === (a.staffUserId && known.has(a.staffUserId) ? a.staffUserId : ANYONE));
  const placed: Placed[] = shown.map((appointment) => {
    const row = slotOf(minutesOf(appointment.startsAt));
    const span = Math.max(1, Math.min(SLOTS - row, Math.ceil(appointment.durationMin / SLOT_MINUTES)));
    return { appointment, column: columnOf(appointment), row, span };
  });
  const taken = new Set(
    placed.flatMap((p) => Array.from({ length: p.span }, (_, i) => `${p.column}:${p.row + i}`)),
  );

  if (users.error || appointments.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(users.error ?? appointments.error)}
      </p>
    );
  }
  if (!users.data || !appointments.data) return <p className="text-ink-2">{t("common:status.loading")}</p>;

  return (
    <div className="overflow-x-auto rounded-card border border-line">
      <div
        role="group"
        aria-label={t("orders:appointments.title")}
        className="grid min-w-max"
        style={{
          gridTemplateColumns: `4rem repeat(${columns.length}, minmax(10rem, 1fr))`,
          gridTemplateRows: `auto repeat(${SLOTS}, 2.75rem)`,
        }}
      >
        <div
          className="sticky top-0 border-b border-line bg-surface-2"
          style={{ gridColumn: 1, gridRow: 1 }}
        />
        {columns.map((c, index) => (
          <div
            key={c.id || "anyone"}
            className="border-s border-b border-line bg-surface-2 p-2 text-center font-semibold"
            style={{ gridColumn: index + 2, gridRow: 1 }}
          >
            {c.name}
          </div>
        ))}
        {Array.from({ length: SLOTS }, (_, row) =>
          row % (60 / SLOT_MINUTES) === 0 ? (
            <div
              key={`hour-${row}`}
              className="border-t border-line pe-2 pt-1 text-end text-sm text-ink-2 tabular-nums"
              style={{ gridColumn: 1, gridRow: `${row + 2} / span ${60 / SLOT_MINUTES}` }}
            >
              <bdi dir="ltr">{hhmm(OPENS + row * SLOT_MINUTES)}</bdi>
            </div>
          ) : null,
        )}
        {columns.flatMap((c, column) =>
          Array.from({ length: SLOTS }, (_, row) => {
            if (taken.has(`${column}:${row}`)) return null;
            const minutes = OPENS + row * SLOT_MINUTES;
            const time = hhmm(minutes);
            return (
              <button
                key={`${c.id || "anyone"}-${row}`}
                type="button"
                aria-label={t("orders:appointments.free", { staff: c.name, time })}
                title={time}
                onClick={() => setOpen({ slot: { staffUserId: c.id === ANYONE ? null : c.id, minutes } })}
                className={cn(
                  "border-s border-line hover:bg-brand-soft focus-visible:outline-3 focus-visible:outline-focus",
                  row % (60 / SLOT_MINUTES) === 0 ? "border-t" : "border-t border-t-line/40",
                )}
                style={{ gridColumn: column + 2, gridRow: row + 2 }}
              />
            );
          }),
        )}
        {placed.map(({ appointment: a, column, row, span }) => (
          <button
            key={a.id}
            type="button"
            onClick={() => setOpen({ appointment: a })}
            className={cn(
              "m-0.5 flex flex-col items-start gap-0.5 overflow-hidden rounded-control border p-2 text-start text-sm",
              a.status === "booked" ? "border-brand bg-brand-soft" : "border-line bg-surface-2 text-ink-2",
            )}
            style={{ gridColumn: column + 2, gridRow: `${row + 2} / span ${span}` }}
          >
            <span className="font-semibold">{a.customerName}</span>
            <span>
              <bdi dir="ltr">{hhmm(minutesOf(a.startsAt))}</bdi> ·{" "}
              {t("orders:appointments.minutes", { count: a.durationMin })}
            </span>
            <ServiceName appointment={a} />
            {a.status !== "booked" ? <span>{t(`orders:appointments.status.${a.status}`)}</span> : null}
          </button>
        ))}
      </div>
      {open ? (
        <AppointmentDialog
          day={day}
          staff={staff}
          {...("slot" in open ? { slot: open.slot } : { appointment: open.appointment })}
          onClose={() => setOpen(null)}
        />
      ) : null}
    </div>
  );
}
