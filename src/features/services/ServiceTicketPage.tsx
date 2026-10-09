import { ArrowLeft, HandCoins, PackageCheck, Undo2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { Splash } from "@/components/splash";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { NativeSelect } from "@/components/ui/native-select";
import { dayToMs } from "@/lib/dates";
import { formatDate, formatDZD } from "@/lib/format";
import { useSession, useUserTiles } from "@/lib/ipc/auth";
import { useAssignTechnician, useServiceTicket, useSetServiceStatus } from "@/lib/ipc/services";
import type { Permission, ServiceStatus, ServiceTicket } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { DepositDialog, RefundDialog } from "./DepositDialog";
import { ItemsEditor } from "./ItemsEditor";
import { ReadyMessage } from "./ReadyMessage";
import { isClosed, nextStatuses } from "./status";

/** What the till receives to hand a ready ticket back (`SaleScreen` reads it from the router). */
export interface PickupState {
  pickup: string;
}

type Open = "deposit" | "refund" | null;

/**
 * A repair or service ticket: what was left and its state, the history of its statuses (who and
 * when), the next statuses it may take, the technician, the parts and labour planned, the deposits
 * held (taken, refunded with `sales.return`), the "ready" message with the customer's consent,
 * and "Remettre au client", which opens the till with the ticket's lines and its deposit.
 */
export function ServiceTicketPage({ id }: { id: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const session = useSession();
  const ticket = useServiceTicket(id);
  const setStatus = useSetServiceStatus();
  const [open, setOpen] = useState<Open>(null);
  const can = (p: Permission) => session.data?.permissions.includes(p) ?? false;

  if (ticket.error) {
    return (
      <div className="flex flex-col gap-4">
        <BackLink />
        <p role="alert" className="text-danger">
          {errorText(ticket.error)}
        </p>
      </div>
    );
  }
  if (!ticket.data) return <Splash inline />;
  const data = ticket.data;
  const closed = isClosed(data.status);
  const heading = t("services:deposit.for", { number: data.number, customer: data.customerName });
  const move = (status: ServiceStatus) => setStatus.mutate({ id: data.id, status });

  return (
    <div className="flex flex-col gap-6">
      <BackLink />
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-3xl font-bold">
          <bdi dir="ltr">{data.number}</bdi> · {data.item}
        </h1>
        <Badge tone={data.status === "ready" ? "ok" : closed ? "neutral" : "info"}>
          {t(`services:status.${data.status}`)}
        </Badge>
      </div>

      {closed ? (
        <p className="rounded-control bg-surface-2 p-3 text-ink-2">{t("services:ticket.closed")}</p>
      ) : (
        <div className="flex flex-col gap-2">
          <span id="ticket-next" className="font-semibold">
            {t("services:ticket.next")}
          </span>
          <div role="group" aria-labelledby="ticket-next" className="flex flex-wrap gap-2">
            {nextStatuses(data.status).map((status) => (
              <Button
                key={status}
                variant={status === "cancelled" || status === "returned_unrepaired" ? "outline" : "secondary"}
                disabled={setStatus.isPending}
                onClick={() => move(status)}
              >
                {t(`services:status.${status}`)}
              </Button>
            ))}
            {data.status === "ready" && can("sales.create") ? (
              <Button
                variant="primary"
                onClick={() => void navigate("/pos", { state: { pickup: data.id } satisfies PickupState })}
              >
                <PackageCheck aria-hidden />
                {t("services:pickup")}
              </Button>
            ) : null}
          </div>
          {setStatus.error ? (
            <p role="alert" className="text-danger">
              {errorText(setStatus.error)}
            </p>
          ) : null}
        </div>
      )}

      <div className="grid gap-6 lg:grid-cols-2">
        <Details ticket={data} />
        <Card>
          <h2 className="text-xl font-bold">{t("services:ticket.timeline")}</h2>
          <ol className="flex flex-col gap-2">
            {data.events.map((event, index) => (
              <li key={`${event.createdAt}-${index}`} className="flex flex-col border-s-4 border-line ps-3">
                <span className="font-semibold">{t(`services:status.${event.status}`)}</span>
                <span className="text-sm text-ink-2">
                  <bdi dir="ltr">{formatDate(event.createdAt, lang, { withTime: true })}</bdi>{" "}
                  {event.userName ? t("services:ticket.by", { name: event.userName }) : null}
                </span>
                {event.note ? <span className="text-sm">{event.note}</span> : null}
              </li>
            ))}
          </ol>
        </Card>
      </div>

      {data.status === "ready" ? (
        <Card>
          <ReadyMessage ticketId={data.id} customerId={data.customerId} lang={data.lang} />
        </Card>
      ) : null}

      <Card>
        <ItemsEditor key={data.id} ticket={data} editable={!closed} />
      </Card>

      <Card>
        <h2 className="text-xl font-bold">{t("services:ticket.deposits")}</h2>
        <p className="text-2xl font-bold">
          <Amount value={data.deposits} />
        </p>
        <div className="flex flex-wrap gap-2">
          {!closed ? (
            <Button variant="secondary" onClick={() => setOpen("deposit")}>
              <HandCoins aria-hidden />
              {t("services:deposit.take")}
            </Button>
          ) : null}
          {data.deposits > 0 && can("sales.return") ? (
            <Button variant="outline" onClick={() => setOpen("refund")}>
              <Undo2 aria-hidden className="rtl:-scale-x-100" />
              {t("services:deposit.refund")}
            </Button>
          ) : null}
        </div>
      </Card>

      {open === "deposit" ? (
        <DepositDialog
          ownerType="service_ticket"
          ownerId={data.id}
          heading={heading}
          onClose={() => setOpen(null)}
        />
      ) : null}
      {open === "refund" ? (
        <RefundDialog
          ownerType="service_ticket"
          ownerId={data.id}
          held={data.deposits}
          heading={heading}
          onClose={() => setOpen(null)}
        />
      ) : null}
    </div>
  );
}

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link
      to="/services"
      className="inline-flex min-h-11 items-center gap-2 self-start font-semibold text-ink-2 hover:text-ink"
    >
      <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
      {t("services:ticket.back")}
    </Link>
  );
}

/** What was left, as the intake slip says, and its technician (changed while it is open). */
function Details({ ticket }: { ticket: ServiceTicket }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const technicians = useUserTiles();
  const assign = useAssignTechnician();
  const closed = isClosed(ticket.status);
  const rows: [string, string | null][] = [
    [t("services:intake.kind"), t(`services:kinds.${ticket.kind}`)],
    [t("services:intake.brand"), ticket.brand],
    [t("services:intake.model"), ticket.model],
    [t("services:intake.accessories"), ticket.accessories],
    [t("services:intake.condition"), ticket.condition],
    [t("services:intake.fault"), ticket.fault],
    [t("services:intake.estimate"), ticket.estimate === null ? null : formatDZD(ticket.estimate, lang)],
    [
      t("services:intake.promised"),
      ticket.promisedOn === null ? null : formatDate(dayToMs(ticket.promisedOn), lang),
    ],
  ];
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("services:ticket.details")}</h2>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
        <dt className="text-ink-2">{t("services:ticket.customer")}</dt>
        <dd className="font-semibold">{ticket.customerName}</dd>
        {ticket.serialNo ? (
          <>
            <dt className="text-ink-2">{t("services:intake.serial")}</dt>
            <dd>
              <bdi dir="ltr" className="tabular-nums">
                {ticket.serialNo}
              </bdi>
            </dd>
          </>
        ) : null}
        {rows.map(([label, value]) =>
          value === null ? null : (
            <div key={label} className="contents">
              <dt className="text-ink-2">{label}</dt>
              <dd>{value}</dd>
            </div>
          ),
        )}
      </dl>
      <Field id="ticket-technician" label={t("services:ticket.technician")}>
        <NativeSelect
          id="ticket-technician"
          disabled={closed || assign.isPending}
          value={ticket.technicianId ?? ""}
          onChange={(event) =>
            assign.mutate({
              id: ticket.id,
              technicianId: event.target.value === "" ? null : event.target.value,
            })
          }
        >
          <option value="">{t("services:intake.noTechnician")}</option>
          {/* the technician named on the ticket, even before the users are read */}
          {ticket.technicianId && !technicians.data?.some((u) => u.id === ticket.technicianId) ? (
            <option value={ticket.technicianId}>{ticket.technicianName}</option>
          ) : null}
          {(technicians.data ?? []).map((u) => (
            <option key={u.id} value={u.id}>
              {u.displayName}
            </option>
          ))}
        </NativeSelect>
      </Field>
      {assign.error ? (
        <p role="alert" className="text-danger">
          {errorText(assign.error)}
        </p>
      ) : null}
    </Card>
  );
}
