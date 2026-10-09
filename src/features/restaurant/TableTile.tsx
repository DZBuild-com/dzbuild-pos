import { ChefHat } from "lucide-react";
import type { CSSProperties, Ref } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { formatDZD } from "@/lib/format";
import type { TableStatus } from "@/lib/ipc/types";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

const STATE_CLASS = {
  free: "border-line bg-surface",
  occupied: "border-brand bg-brand-soft",
  billed: "border-warn bg-warn-soft",
} as const;

/** A table of the floor (RST-002): its code, its covers, the minutes since it opened and since its
 * last kitchen ticket (the backend's, so a wrong PC clock does not lie), its ticket's total, and a
 * "non imprimé" badge while a kitchen ticket waits for its reprint; coloured by its state.
 * `tabIndex` is the floor's roving focus: one table of the grid is the Tab stop, the arrows move
 * between them. */
export function TableTile({
  status,
  onOpen,
  style,
  ref,
  tabIndex,
  onFocus,
}: {
  status: TableStatus;
  onOpen: () => void;
  style?: CSSProperties;
  ref?: Ref<HTMLButtonElement>;
  tabIndex?: number;
  onFocus?: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const { table, state, tickets } = status;
  const covers = tickets.reduce((sum, ticket) => sum + ticket.covers, 0);
  const minutes = Math.max(0, ...tickets.map((ticket) => ticket.minutesOpen));
  const kitchen = tickets.flatMap((ticket) =>
    ticket.minutesSinceKitchen === null ? [] : [ticket.minutesSinceKitchen],
  );
  const sinceKitchen = kitchen.length > 0 ? Math.min(...kitchen) : null;
  const unprinted = tickets.some((ticket) => ticket.unprintedKitchen > 0);
  // final review: dishes the kitchen has not been sent yet
  const unsent = tickets.some((ticket) => ticket.unsentLines > 0);
  // one ticket shows its total; a table split between guests shows how many tickets it has
  const single = tickets.length === 1 ? tickets[0] : undefined;
  const label =
    state === "free"
      ? t("restaurant:floor.freeLabel", { code: table.code, seats: table.seats })
      : t("restaurant:floor.tableLabel", {
          code: table.code,
          state: t(`restaurant:floor.${state}`),
          covers,
          minutes,
          total: single
            ? formatDZD(single.totalTtc, lang)
            : t("restaurant:floor.tickets", { count: tickets.length }),
        });
  return (
    <button
      ref={ref}
      type="button"
      data-state={state}
      aria-label={label}
      tabIndex={tabIndex}
      onClick={onOpen}
      onFocus={onFocus}
      style={style}
      className={cn(
        "flex min-h-24 min-w-24 flex-col items-start justify-between gap-1 border-2 p-3 text-start",
        table.shape === "round" ? "rounded-[2rem]" : "rounded-card",
        STATE_CLASS[state],
      )}
    >
      <span className="flex w-full items-start justify-between gap-2">
        <span className="text-2xl font-bold">{table.code}</span>
        {unprinted ? (
          <span className="rounded-full bg-danger-soft px-2 text-sm font-semibold text-danger">
            {t("restaurant:floor.notPrinted")}
          </span>
        ) : unsent ? (
          <span className="rounded-full bg-warn-soft px-2 text-sm font-semibold text-warn">
            {t("restaurant:ticket.unsent")}
          </span>
        ) : null}
      </span>
      {state === "free" ? (
        <span className="text-ink-2">{t("restaurant:floor.seats", { count: table.seats })}</span>
      ) : (
        <>
          <span className="flex flex-wrap items-center gap-x-3 text-sm font-semibold">
            <span>{t("restaurant:floor.covers", { count: covers })}</span>
            <span className="tabular-nums">{t("restaurant:floor.minutes", { count: minutes })}</span>
            {sinceKitchen !== null ? (
              <span className="flex items-center gap-1 text-ink-2 tabular-nums">
                <ChefHat aria-hidden className="size-4" />
                {t("restaurant:floor.minutes", { count: sinceKitchen })}
              </span>
            ) : null}
          </span>
          {single ? (
            <Amount value={single.totalTtc} className="text-lg font-bold" />
          ) : (
            <span className="font-semibold">{t("restaurant:floor.tickets", { count: tickets.length })}</span>
          )}
        </>
      )}
    </button>
  );
}
