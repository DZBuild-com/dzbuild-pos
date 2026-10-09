import { ArrowLeft, HandCoins, Printer } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Amount } from "@/components/amount";
import { Splash } from "@/components/splash";
import { StatTile } from "@/components/stat-tile";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { dayToMs } from "@/lib/dates";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useCustomer } from "@/lib/ipc/customers";
import { usePlan } from "@/lib/ipc/instalments";
import type { DueState, Permission } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { CollectDialog } from "./CollectDialog";
import { ConsentLines } from "./ConsentLines";
import { ContractPrintDialog } from "./ContractPrintDialog";
import { AddGuaranteeDialog, GuaranteeList } from "./Guarantees";
import { daysLate, dueTone, nextDue } from "./kinds";

type Open = "collect" | "guarantee" | "contract" | null;

/** A due's state as its badge says it: "En retard de 36 jours" for a late one. */
export function DueBadge({ due }: { due: DueState }) {
  const { t } = useTranslation();
  return (
    <Badge tone={dueTone(due.status)}>
      {t(`instalments:plan.status.${due.status}`, { count: due.daysLate })}
    </Badge>
  );
}

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link
      to="/instalments"
      className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("instalments:title")}
    </Link>
  );
}

/**
 * A signed contract (TMP-003/004): what was financed, paid and left, each due with its state
 * (payée, partielle, due, en retard N jours, à venir), the supporting documents (+ one more), the
 * customer's consent for the reminders, "Encaisser une échéance" and "Réimprimer le contrat".
 * Every figure is the backend's.
 */
export function PlanPage({ id }: { id: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const session = useSession();
  const plan = usePlan(id);
  const customer = useCustomer(plan.data?.customerId ?? null);
  const [open, setOpen] = useState<Open>(null);
  const can = (p: Permission) => session.data?.permissions.includes(p) ?? false;

  if (plan.error) {
    return (
      <div className="flex flex-col gap-4">
        <BackLink />
        <p role="alert" className="text-danger">
          {errorText(plan.error)}
        </p>
      </div>
    );
  }
  if (!plan.data) return <Splash inline />;
  const data = plan.data;
  const next = nextDue(data);
  const late = daysLate(data);

  return (
    <div className="flex flex-col gap-6">
      <BackLink />
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-3xl font-bold">
          <bdi dir="ltr">{data.number}</bdi> · {data.customerName}
        </h1>
        {late > 0 ? <Badge tone="danger">{t("instalments:plan.late")}</Badge> : null}
      </div>
      <p className="flex flex-wrap gap-x-4 gap-y-1 text-ink-2">
        <span>{t("instalments:plan.signedOn", { date: formatDate(dayToMs(data.businessDate), lang) })}</span>
        <Link to={`/documents/${data.documentId}`} className="font-semibold text-ink underline">
          {t("instalments:plan.ticket", { number: data.documentNumber })}
        </Link>
        {can("customers.view") ? (
          <Link to={`/customers/${data.customerId}`} className="font-semibold text-ink underline">
            {t("instalments:plan.customerFile")}
          </Link>
        ) : null}
      </p>

      <div className="grid gap-3 sm:grid-cols-3 xl:grid-cols-6">
        <StatTile label={t("instalments:plan.total")} value={<Amount value={data.total} />} />
        <StatTile label={t("instalments:plan.down")} value={<Amount value={data.downPayment} />} />
        <StatTile label={t("instalments:plan.financed")} value={<Amount value={data.financed} />} />
        <StatTile label={t("instalments:plan.paid")} value={<Amount value={data.paid} />} />
        <StatTile label={t("instalments:plan.left")} value={<Amount value={data.remaining} />} />
        <StatTile
          label={t("instalments:plan.next")}
          value={next ? <bdi dir="ltr">{formatDate(dayToMs(next.dueOn), lang)}</bdi> : "—"}
        />
      </div>

      <div className="flex flex-wrap gap-2">
        {can("credit.collect") ? (
          <Button variant="primary" disabled={data.remaining <= 0} onClick={() => setOpen("collect")}>
            <HandCoins aria-hidden />
            {t("instalments:plan.collect")}
          </Button>
        ) : null}
        {can("sales.create") ? (
          <Button variant="secondary" onClick={() => setOpen("contract")}>
            <Printer aria-hidden />
            {t("instalments:plan.reprint")}
          </Button>
        ) : null}
      </div>

      <section className="flex flex-col gap-3">
        <h2 className="text-xl font-bold">{t("instalments:plan.dues")}</h2>
        <table aria-label={t("instalments:plan.dues")} className="w-full max-w-3xl border-collapse">
          <thead>
            <tr className="border-b border-line text-ink-2">
              <th scope="col" className="py-2 text-start font-semibold">
                {t("instalments:plan.seq")}
              </th>
              <th scope="col" className="py-2 text-start font-semibold">
                {t("instalments:plan.dueOn")}
              </th>
              <th scope="col" className="py-2 text-end font-semibold">
                {t("instalments:plan.amount")}
              </th>
              <th scope="col" className="py-2 text-end font-semibold">
                {t("instalments:plan.paid")}
              </th>
              <th scope="col" className="py-2 ps-4 text-start font-semibold">
                {t("instalments:plan.state")}
              </th>
            </tr>
          </thead>
          <tbody>
            {data.dues.map((due) => (
              <tr key={due.seq} className="border-b border-line even:bg-surface-2">
                <td className="py-2 tabular-nums">{due.seq}</td>
                <td className="py-2">
                  <bdi dir="ltr">{formatDate(dayToMs(due.dueOn), lang)}</bdi>
                </td>
                <td className="py-2 text-end">
                  <Amount value={due.amount} />
                </td>
                <td className="py-2 text-end">
                  <Amount value={due.paid} />
                </td>
                <td className="py-2 ps-4">
                  <DueBadge due={due} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <h2 className="text-xl font-bold">{t("instalments:guarantees.title")}</h2>
          {can("customers.manage") ? (
            <Button variant="outline" onClick={() => setOpen("guarantee")}>
              {t("instalments:guarantees.add")}
            </Button>
          ) : null}
        </div>
        <GuaranteeList items={data.guarantees} />
        <p className="text-sm text-ink-2">{t("instalments:guarantees.noCheque")}</p>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-xl font-bold">{t("instalments:reminders.title")}</h2>
        <ConsentLines customerId={data.customerId} />
      </section>

      {open === "collect" ? <CollectDialog plan={data} onClose={() => setOpen(null)} /> : null}
      {open === "guarantee" ? <AddGuaranteeDialog planId={data.id} onClose={() => setOpen(null)} /> : null}
      {open === "contract" ? (
        <ContractPrintDialog
          planId={data.id}
          lang={customer.data?.lang ?? lang}
          onClose={() => setOpen(null)}
        />
      ) : null}
    </div>
  );
}
