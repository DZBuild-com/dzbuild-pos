import { ArrowLeft } from "lucide-react";
import { type FormEvent, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DateRangePicker } from "@/components/date-range";
import { NumericInput } from "@/components/numeric-input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { type DateRange, dayToMs, rangeOf, today } from "@/lib/dates";
import { formatAmountInput, formatDZD, formatDate, formatPercentInput, parsePercentBp } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useSetProfile, useShifts, useStaff, useStaffEntries } from "@/lib/ipc/staff";
import type { Permission, ShiftRow, StaffEntry, StaffProfile } from "@/lib/ipc/types";
import { parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { AdvanceDialog, DeductDialog, RepayDialog } from "./AdvanceDialog";

type Open = "advance" | "repay" | "deduct" | null;

/** The highest commission a profile takes (pos-core `staff::COMMISSION_MAX_BP`, 50 %). */
const COMMISSION_MAX_BP = 5_000;

/** `/staff/:userId` (EMP lite): the employee's file, what they owe on their advances with its
 * ledger, the advance, repayment and deduction, and their shifts of a period. */
export function StaffPage({ userId }: { userId: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const session = useSession();
  const staff = useStaff();
  const back = (
    <Link
      to="/staff"
      className="inline-flex min-h-11 items-center gap-2 font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("staff:back")}
    </Link>
  );
  if (staff.error) {
    return (
      <div className="flex flex-col gap-4">
        {back}
        <p role="alert" className="text-danger">
          {errorText(staff.error)}
        </p>
      </div>
    );
  }
  // the actions depend on the user: the page waits for the session too
  if (!staff.data || !session.data) {
    return (
      <div className="flex flex-col gap-4">
        {back}
        <p className="text-ink-2">{t("common:status.loading")}</p>
      </div>
    );
  }
  const employee = staff.data.find((p) => p.userId === userId);
  if (!employee) {
    return (
      <div className="flex flex-col gap-4">
        {back}
        <p role="status" className="text-ink-2">
          {t("staff:notFound")}
        </p>
      </div>
    );
  }
  return (
    <EmployeeFile
      key={employee.userId}
      employee={employee}
      permissions={session.data.permissions}
      back={back}
    />
  );
}

function EmployeeFile({
  employee,
  permissions,
  back,
}: {
  employee: StaffProfile;
  permissions: readonly Permission[];
  back: ReactNode;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const can = (p: Permission) => permissions.includes(p);
  const [open, setOpen] = useState<Open>(null);
  const owes = employee.owed > 0;
  return (
    <div className="flex flex-col gap-6">
      {back}
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="flex flex-wrap items-center gap-3 text-3xl font-bold">
            {employee.displayName}
            {employee.active ? null : <Badge>{t("staff:inactive")}</Badge>}
          </h1>
          <p className="text-ink-2">{employee.roleName}</p>
          <p className="text-xl font-bold" role="status">
            {t("staff:ledger.owed", { amount: formatDZD(employee.owed, lang) })}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {can("cash.paid_in_out") ? (
            <>
              <Button variant="primary" onClick={() => setOpen("advance")}>
                {t("staff:advance.give")}
              </Button>
              <Button variant="outline" disabled={!owes} onClick={() => setOpen("repay")}>
                {t("staff:advance.repay")}
              </Button>
            </>
          ) : null}
          {can("settings.users") ? (
            <Button variant="outline" disabled={!owes} onClick={() => setOpen("deduct")}>
              {t("staff:advance.deduct")}
            </Button>
          ) : null}
        </div>
      </header>
      {can("settings.users") ? <ProfileCard employee={employee} /> : null}
      {can("settings.users") ? <Ledger userId={employee.userId} /> : null}
      {can("reports.cash") ? <Shifts userId={employee.userId} /> : null}
      {open === "advance" ? <AdvanceDialog employee={employee} onClose={() => setOpen(null)} /> : null}
      {open === "repay" ? <RepayDialog employee={employee} onClose={() => setOpen(null)} /> : null}
      {open === "deduct" ? <DeductDialog employee={employee} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}

const text = (value: string) => (value.trim() === "" ? null : value.trim());

/** Phone, hire date, salary, commission rate and note (`staff_profile_set`). */
function ProfileCard({ employee }: { employee: StaffProfile }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const save = useSetProfile();
  const [phone, setPhone] = useState(employee.phone ?? "");
  const [hireDate, setHireDate] = useState(employee.hireDate ?? "");
  const [salary, setSalary] = useState(
    employee.monthlySalary === null ? "" : formatAmountInput(employee.monthlySalary, lang),
  );
  const [commission, setCommission] = useState(formatPercentInput(employee.commissionBp, lang));
  const [note, setNote] = useState(employee.note ?? "");
  const [tried, setTried] = useState(false);
  const salaryValue = salary.trim() === "" ? null : parseAmount(salary);
  const salaryOk = salary.trim() === "" || (salaryValue !== null && salaryValue >= 0);
  const commissionBp = parsePercentBp(commission.trim() === "" ? "0" : commission);
  const commissionOk = commissionBp !== null && commissionBp <= COMMISSION_MAX_BP;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (!salaryOk || commissionBp === null || !commissionOk) return;
    save.mutate(
      {
        userId: employee.userId,
        input: {
          phone: text(phone),
          hireDate: text(hireDate),
          monthlySalary: salaryValue,
          commissionBp,
          note: text(note),
        },
      },
      { onSuccess: () => toast.success(t("staff:profile.saved")) },
    );
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("staff:profile.title")}</h2>
      <form noValidate onSubmit={submit} className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Field id="staff-phone" label={t("staff:profile.phone")}>
            <Input
              id="staff-phone"
              dir="ltr"
              inputMode="tel"
              maxLength={20}
              value={phone}
              onChange={(event) => setPhone(event.target.value)}
            />
          </Field>
          <Field id="staff-hire-date" label={t("staff:profile.hireDate")}>
            <Input
              id="staff-hire-date"
              type="date"
              dir="ltr"
              value={hireDate}
              onChange={(event) => setHireDate(event.target.value)}
            />
          </Field>
          <Field
            id="staff-salary"
            label={t("staff:profile.salary")}
            error={tried && !salaryOk ? t("validation:amountFormat") : undefined}
          >
            <NumericInput id="staff-salary" value={salary} onValueChange={setSalary} />
          </Field>
          <Field
            id="staff-commission"
            label={t("staff:profile.commission")}
            hint={t("staff:profile.commissionHint")}
            error={tried && !commissionOk ? t("errors:staff_amount_invalid") : undefined}
          >
            <Input
              id="staff-commission"
              dir="ltr"
              inputMode="decimal"
              value={commission}
              onChange={(event) => setCommission(event.target.value)}
            />
          </Field>
        </div>
        <Field id="staff-note" label={t("staff:profile.note")} optional={t("customers:form.optional")}>
          <Input
            id="staff-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(event) => setNote(event.target.value)}
          />
        </Field>
        {save.error ? (
          <p role="alert" className="text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
        <Button type="submit" variant="secondary" className="self-start" disabled={save.isPending}>
          {t("staff:profile.save")}
        </Button>
      </form>
    </Card>
  );
}

/** The advances ledger, newest first, each amount signed as it moves what is owed. */
function Ledger({ userId }: { userId: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const entries = useStaffEntries(userId);
  const rows: StaffEntry[] = entries.data ?? [];
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("staff:ledger.title")}</h2>
      {entries.error ? (
        <p role="alert" className="text-danger">
          {errorText(entries.error)}
        </p>
      ) : rows.length === 0 ? (
        <p className="text-ink-2">
          {entries.isPending ? t("common:status.loading") : t("staff:ledger.empty")}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
            <thead className="text-sm text-ink-2">
              <tr className="h-10">
                <th scope="col" className="text-start font-semibold">
                  {t("staff:ledger.date")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("staff:ledger.kind")}
                </th>
                <th scope="col" className="text-end font-semibold">
                  {t("staff:ledger.amount")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("staff:ledger.method")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("staff:ledger.note")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("staff:ledger.by")}
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((entry) => (
                <tr key={entry.id} className="h-10 border-t border-line even:bg-surface-2">
                  <td>
                    <bdi dir="ltr">{formatDate(dayToMs(entry.businessDate), lang)}</bdi>
                  </td>
                  <td>
                    {t(`staff:ledger.${entry.kind}`)}
                    {entry.month ? (
                      <>
                        {" "}
                        <bdi dir="ltr" className="text-ink-2">
                          {entry.month}
                        </bdi>
                      </>
                    ) : null}
                  </td>
                  <td className="text-end">
                    <Amount value={entry.amount} className="font-semibold" />
                  </td>
                  <td>{entry.method ? t(`payments:tenders.${entry.method}`) : null}</td>
                  <td dir="auto">{entry.note}</td>
                  <td>{entry.createdByName}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

/** The cash sessions of the period (`staff_shifts`), of `userId` or of everyone (`null`). */
export function Shifts({ userId }: { userId: string | null }) {
  const { t } = useTranslation();
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("staff:shifts.title")}</h2>
      <DateRangePicker value={range} onChange={setRange} />
      <ShiftTable from={range.from} to={range.to} userId={userId} />
    </Card>
  );
}

function ShiftTable({ from, to, userId }: { from: string; to: string; userId: string | null }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const shifts = useShifts(from, to, userId);
  const rows: ShiftRow[] = shifts.data ?? [];
  if (shifts.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(shifts.error)}
      </p>
    );
  }
  if (rows.length === 0) {
    return (
      <p className="text-ink-2">{shifts.isPending ? t("common:status.loading") : t("staff:shifts.empty")}</p>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse">
        <thead className="text-sm text-ink-2">
          <tr className="h-10">
            {userId === null ? (
              <th scope="col" className="text-start font-semibold">
                {t("staff:columns.name")}
              </th>
            ) : null}
            <th scope="col" className="text-start font-semibold">
              {t("staff:shifts.terminal")}
            </th>
            <th scope="col" className="text-start font-semibold">
              {t("staff:shifts.opened")}
            </th>
            <th scope="col" className="text-start font-semibold">
              {t("staff:shifts.closed")}
            </th>
            <th scope="col" className="text-end font-semibold">
              {t("staff:shifts.duration")}
            </th>
            <th scope="col" className="text-end font-semibold">
              {t("staff:shifts.tickets")}
            </th>
            <th scope="col" className="text-end font-semibold">
              {t("staff:shifts.sales")}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.sessionId} className="h-10 border-t border-line even:bg-surface-2">
              {userId === null ? <td>{row.name}</td> : null}
              <td>
                <bdi dir="ltr">{row.terminalCode}</bdi>
              </td>
              <td>
                <bdi dir="ltr">{formatDate(row.openedAt, lang, { withTime: true })}</bdi>
              </td>
              <td>
                {row.closedAt === null ? (
                  <Badge tone="ok">{t("staff:shifts.running")}</Badge>
                ) : (
                  <bdi dir="ltr">{formatDate(row.closedAt, lang, { withTime: true })}</bdi>
                )}
              </td>
              <td className="text-end tabular-nums">
                {t("staff:shifts.hours", {
                  hours: Math.floor(row.minutes / 60),
                  minutes: String(row.minutes % 60).padStart(2, "0"),
                })}
              </td>
              <td className="text-end tabular-nums">{row.tickets}</td>
              <td className="text-end">
                <Amount value={row.salesTtc} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
