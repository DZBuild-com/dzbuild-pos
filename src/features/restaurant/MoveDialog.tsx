import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Dialog } from "@/components/ui/dialog";
import { useDiningBoard, useDiningCounter, useMergeTickets, useTransferTicket } from "@/lib/ipc/restaurant";
import type { Ticket, TicketSummary } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { counterName } from "./CounterTickets";

/** RST-004: "Transférer" moves the ticket to a free table; "Fusionner" moves its lines into
 * another open ticket (lines move, never copy). `onMoved` gets the ticket the waiter goes on with. */
export function MoveDialog({
  ticket,
  mode,
  onClose,
  onMoved,
}: {
  ticket: Ticket;
  mode: "transfer" | "merge";
  onClose: () => void;
  onMoved: (ticketId: string) => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const board = useDiningBoard(null);
  const counter = useDiningCounter(mode === "merge");
  const transfer = useTransferTicket();
  const merge = useMergeTickets();
  const error = transfer.error ?? merge.error;
  const busy = transfer.isPending || merge.isPending;
  const free = (board.data ?? []).filter((s) => s.state === "free" && s.table.id !== ticket.tableId);
  const others: TicketSummary[] = [
    ...(board.data ?? []).flatMap((s) => s.tickets),
    ...(counter.data ?? []),
  ].filter((other) => other.id !== ticket.id);
  const nameOf = (other: TicketSummary) =>
    other.tableCode === null
      ? counterName(other, t)
      : `${t("restaurant:open.title", { code: other.tableCode })} · ${other.waiterName}`;
  const tile =
    "flex min-h-14 w-full items-center justify-between gap-3 rounded-card border border-line bg-surface-2 p-3 text-start font-semibold hover:border-brand disabled:opacity-50";
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={mode === "transfer" ? t("restaurant:move.transfer") : t("restaurant:move.merge")}
    >
      {mode === "transfer" ? (
        free.length === 0 && board.isSuccess ? (
          <p className="text-ink-2">{t("restaurant:move.noFreeTable")}</p>
        ) : (
          <div className="grid grid-cols-3 gap-2 sm:grid-cols-4">
            {free.map((s) => (
              <button
                key={s.table.id}
                type="button"
                disabled={busy}
                className={tile}
                onClick={() =>
                  transfer.mutate(
                    { ticketId: ticket.id, tableId: s.table.id },
                    { onSuccess: (moved) => onMoved(moved.id) },
                  )
                }
              >
                {s.table.code}
              </button>
            ))}
          </div>
        )
      ) : others.length === 0 && board.isSuccess ? (
        <p className="text-ink-2">{t("restaurant:move.noTicket")}</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {others.map((other) => (
            <li key={other.id}>
              <button
                type="button"
                disabled={busy}
                className={tile}
                onClick={() =>
                  merge.mutate(
                    { fromId: ticket.id, intoId: other.id },
                    { onSuccess: (into) => onMoved(into.id) },
                  )
                }
              >
                <span>{nameOf(other)}</span>
                <Amount value={other.totalTtc} className="font-bold" />
              </button>
            </li>
          ))}
        </ul>
      )}
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </Dialog>
  );
}
