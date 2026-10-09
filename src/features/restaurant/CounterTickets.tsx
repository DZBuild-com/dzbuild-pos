import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { formatDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { useDiningCounter, useOpenTicket } from "@/lib/ipc/restaurant";
import { useSetting } from "@/lib/ipc/settings";
import type { OpenInput, TicketSummary } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The predefined tickets of the counter (`restaurant.predefined_tickets`: "Comptoir",
 * "Terrasse"…), as the settings hold them. */
function labelsOf(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

/** What a ticket without a table is called: its daily number and its kind, or its label. */
export function counterName(ticket: Pick<TicketSummary, "mode" | "orderNo" | "label">, t: TFunction): string {
  const kind =
    ticket.mode === "takeaway"
      ? t("restaurant:floor.takeaway")
      : ticket.mode === "delivery"
        ? t("restaurant:floor.delivery")
        : (ticket.label ?? "");
  return ticket.orderNo === null
    ? kind
    : `${t("restaurant:floor.order", { number: ticket.orderNo })} · ${kind}`;
}

/** RST-003: the counter column of the floor. A takeaway ticket opens at once, a delivery asks its
 * address and phone, each predefined ticket ("Comptoir") is one button; under them the live
 * takeaway, delivery and counter tickets with their numbers, totals and minutes. */
export function CounterTickets({
  lang,
  onOpen,
  onDelivery,
}: {
  /** The language the new tickets are printed in. */
  lang: Lang;
  onOpen: (ticketId: string) => void;
  onDelivery: () => void;
}) {
  const { t } = useTranslation();
  const uiLang = useLang();
  const errorText = useErrorText();
  const counter = useDiningCounter();
  const labels = labelsOf(useSetting("restaurant.predefined_tickets").data);
  const open = useOpenTicket();
  const start = (input: Pick<OpenInput, "mode"> & Partial<OpenInput>) =>
    open.mutate(
      {
        tableId: null,
        label: null,
        covers: 0,
        customerId: null,
        deliveryAddress: null,
        priceLevelId: null,
        lang,
        ...input,
      },
      { onSuccess: (ticket) => onOpen(ticket.id) },
    );
  const tickets = counter.data ?? [];
  return (
    <aside
      aria-labelledby="counter-title"
      className="flex w-72 shrink-0 flex-col gap-3 overflow-y-auto border-s border-line bg-surface p-4 lg:w-80"
    >
      <h2 id="counter-title" className="text-xl font-bold">
        {t("restaurant:floor.counter")}
      </h2>
      <div className="grid grid-cols-2 gap-2">
        <Button
          size="lg"
          className="min-h-14"
          disabled={open.isPending}
          onClick={() => start({ mode: "takeaway" })}
        >
          {t("restaurant:floor.takeaway")}
        </Button>
        <Button size="lg" className="min-h-14" disabled={open.isPending} onClick={onDelivery}>
          {t("restaurant:floor.delivery")}
        </Button>
        {labels.map((label) => (
          <Button
            key={label}
            variant="outline"
            size="lg"
            className="min-h-14"
            disabled={open.isPending}
            onClick={() => start({ mode: "dine_in", label, covers: 1 })}
          >
            {label}
          </Button>
        ))}
      </div>
      {open.error ? (
        <p role="alert" className="text-danger">
          {errorText(open.error)}
        </p>
      ) : null}
      {tickets.length === 0 && counter.isSuccess ? (
        <p className="text-ink-2">{t("restaurant:floor.noCounter")}</p>
      ) : null}
      <ul className="flex flex-col gap-2">
        {tickets.map((ticket) => {
          const name = counterName(ticket, t);
          return (
            <li key={ticket.id}>
              <button
                type="button"
                onClick={() => onOpen(ticket.id)}
                aria-label={t("restaurant:floor.counterLabel", {
                  name,
                  minutes: ticket.minutesOpen,
                  total: formatDZD(ticket.totalTtc, uiLang),
                })}
                className="flex min-h-14 w-full flex-col gap-1 rounded-card border border-line bg-surface-2 p-3 text-start hover:border-brand"
              >
                <span className="flex w-full justify-between gap-2 font-semibold">
                  <span>{name}</span>
                  <span className="text-ink-2 tabular-nums">
                    {t("restaurant:floor.minutes", { count: ticket.minutesOpen })}
                  </span>
                </span>
                {ticket.unsentLines > 0 ? (
                  <span className="self-start rounded-full bg-warn-soft px-2 text-sm font-semibold text-warn">
                    {t("restaurant:ticket.unsent")}
                  </span>
                ) : null}
                {ticket.mode === "delivery" && ticket.label ? (
                  <bdi dir="ltr" className="text-sm text-ink-2">
                    {ticket.label}
                  </bdi>
                ) : null}
                <Amount value={ticket.totalTtc} className="font-bold" />
              </button>
            </li>
          );
        })}
      </ul>
    </aside>
  );
}
