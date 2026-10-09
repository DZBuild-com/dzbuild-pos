import { useTranslation } from "react-i18next";
import { Link, Route, Routes, useNavigate, useParams } from "react-router";
import { Amount } from "@/components/amount";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useStaff } from "@/lib/ipc/staff";
import type { Permission, Session } from "@/lib/ipc/types";
import { formatRate } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { Shifts, StaffPage } from "./StaffPage";

/** `/staff/*` (EMP lite): the staff list, and `/staff/:userId` the employee's file. */
export function StaffScreen({ session }: { session: Session }) {
  return (
    <Routes>
      <Route index element={<StaffList session={session} />} />
      <Route path=":userId" element={<StaffRoute />} />
    </Routes>
  );
}

function StaffRoute() {
  const { userId = "" } = useParams();
  return <StaffPage userId={userId} />;
}

/** The users with their profile and what they owe (`settings.users`: the salary is theirs to
 * see), the staff reports, and everyone's shifts of a period (`reports.cash`). */
function StaffList({ session }: { session: Session }) {
  const { t } = useTranslation();
  const can = (p: Permission) => session.permissions.includes(p);
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <h1 className="text-3xl font-bold">{t("staff:title")}</h1>
        <p className="text-ink-2">{t("staff:intro")}</p>
      </div>
      {can("settings.users") ? <Employees /> : null}
      {can("reports.sales") || can("reports.cash") ? (
        <Card>
          <h2 className="text-xl font-bold">{t("staff:reports.title")}</h2>
          <div className="flex flex-wrap gap-2">
            {can("reports.sales") ? (
              <Link
                to="/reports/staff_commissions"
                className="inline-flex min-h-11 items-center rounded-control border border-line px-4 font-semibold hover:bg-surface-2"
              >
                {t("staff:reports.commissions")}
              </Link>
            ) : null}
            {can("reports.cash") ? (
              <Link
                to="/reports/staff_advances"
                className="inline-flex min-h-11 items-center rounded-control border border-line px-4 font-semibold hover:bg-surface-2"
              >
                {t("staff:reports.advances")}
              </Link>
            ) : null}
          </div>
        </Card>
      ) : null}
      {can("reports.cash") ? <Shifts userId={null} /> : null}
    </div>
  );
}

function Employees() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const staff = useStaff();
  if (staff.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(staff.error)}
      </p>
    );
  }
  const rows = staff.data ?? [];
  if (rows.length === 0) {
    return (
      <p className="rounded-card border border-dashed border-line p-6 text-center text-ink-2">
        {staff.isPending ? t("common:status.loading") : t("staff:empty")}
      </p>
    );
  }
  return (
    <div className="overflow-x-auto rounded-card border border-line bg-surface">
      <table className="w-full border-collapse">
        <thead className="bg-surface-2 text-sm text-ink-2">
          <tr className="h-10">
            <th scope="col" className="px-3 text-start font-semibold">
              {t("staff:columns.name")}
            </th>
            <th scope="col" className="px-3 text-start font-semibold">
              {t("staff:columns.role")}
            </th>
            <th scope="col" className="px-3 text-start font-semibold max-lg:hidden">
              {t("staff:columns.phone")}
            </th>
            <th scope="col" className="px-3 text-start font-semibold max-lg:hidden">
              {t("staff:columns.hireDate")}
            </th>
            <th scope="col" className="px-3 text-end font-semibold">
              {t("staff:columns.salary")}
            </th>
            <th scope="col" className="px-3 text-end font-semibold">
              {t("staff:columns.commission")}
            </th>
            <th scope="col" className="px-3 text-end font-semibold">
              {t("staff:columns.owed")}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => (
            <tr
              key={p.userId}
              className="h-12 cursor-pointer border-t border-line hover:bg-brand-soft"
              onClick={() => void navigate(`/staff/${p.userId}`)}
            >
              <th scope="row" className="px-3 text-start font-semibold">
                <Link to={`/staff/${p.userId}`} className="hover:underline">
                  {p.displayName}
                </Link>
                {p.active ? null : (
                  <>
                    {" "}
                    <Badge>{t("staff:inactive")}</Badge>
                  </>
                )}
              </th>
              <td className="px-3">{p.roleName}</td>
              <td className="px-3 max-lg:hidden">{p.phone ? <bdi dir="ltr">{p.phone}</bdi> : null}</td>
              <td className="px-3 max-lg:hidden">
                {p.hireDate ? <bdi dir="ltr">{formatDate(dayToMs(p.hireDate), lang)}</bdi> : null}
              </td>
              <td className="px-3 text-end">
                {p.monthlySalary === null ? null : <Amount value={p.monthlySalary} />}
              </td>
              <td className="px-3 text-end">
                <bdi dir="ltr" className="tabular-nums">
                  {formatRate(p.commissionBp, lang)}
                </bdi>
              </td>
              <td className="px-3 text-end">
                <Amount value={p.owed} className={p.owed > 0 ? "font-bold" : "text-ink-2"} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
