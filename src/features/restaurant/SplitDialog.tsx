import { useState } from "react";
import { useTranslation } from "react-i18next";
import { RadioCards } from "@/components/radio-cards";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { useSplitTicket } from "@/lib/ipc/restaurant";
import type { SplitBy, Ticket } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** RST-005: moves chosen lines (with their options), or one seat's lines, to a new ticket at the
 * same table; nothing is copied. `onSplit` gets the new ticket. */
export function SplitDialog({
  ticket,
  onClose,
  onSplit,
}: {
  ticket: Ticket;
  onClose: () => void;
  onSplit: (to: Ticket) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const split = useSplitTicket();
  const [mode, setMode] = useState<"lines" | "seat">("lines");
  const [lineIds, setLineIds] = useState<string[]>([]);
  const [seat, setSeat] = useState<number | null>(null);
  const mains = ticket.lines.filter((l) => l.parentLineId === null);
  const seats = [...new Set(mains.flatMap((l) => (l.seat === null ? [] : [l.seat])))].sort((a, b) => a - b);
  const by: SplitBy | null =
    mode === "lines"
      ? lineIds.length > 0 && lineIds.length < mains.length
        ? { by: "lines", lineIds }
        : null
      : seat === null
        ? null
        : { by: "seat", seat };
  const toggle = (id: string, on: boolean) =>
    setLineIds((now) => (on ? [...now, id] : now.filter((x) => x !== id)));
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("restaurant:split.title")}
      footer={
        <>
          <Button variant="outline" size="lg" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            size="lg"
            disabled={by === null || split.isPending}
            onClick={() => {
              if (by) split.mutate({ ticketId: ticket.id, by }, { onSuccess: (done) => onSplit(done.to) });
            }}
          >
            {t("restaurant:split.submit")}
          </Button>
        </>
      }
    >
      <RadioCards
        legend={t("restaurant:split.how")}
        name="split-mode"
        value={mode}
        onChange={setMode}
        options={[
          { value: "lines", label: t("restaurant:split.byLines") },
          { value: "seat", label: t("restaurant:split.bySeat") },
        ]}
      />
      {mode === "lines" ? (
        <ul className="flex flex-col gap-2">
          {mains.map((line) => (
            <li key={line.id}>
              <label className="flex min-h-11 items-center gap-3 rounded-control border border-line px-3">
                <Checkbox
                  checked={lineIds.includes(line.id)}
                  onCheckedChange={(v) => toggle(line.id, v === true)}
                />
                <span className="flex-1">{line.name}</span>
                <bdi dir="ltr" className="tabular-nums">
                  × {formatQty(line.qty, lang)}
                </bdi>
              </label>
            </li>
          ))}
        </ul>
      ) : seats.length === 0 ? (
        <p className="text-ink-2">{t("restaurant:split.noSeats")}</p>
      ) : (
        <div role="radiogroup" aria-label={t("restaurant:ticket.seat")} className="flex flex-wrap gap-2">
          {seats.map((s) => (
            <button
              key={s}
              type="button"
              role="radio"
              aria-checked={seat === s}
              onClick={() => setSeat(s)}
              className={cn(
                "min-h-14 min-w-14 rounded-control border-2 px-4 text-lg font-bold",
                seat === s ? "border-brand bg-brand-soft" : "border-line bg-surface-2",
              )}
            >
              {t("restaurant:ticket.seatNumber", { number: s })}
            </button>
          ))}
        </div>
      )}
      {split.error ? (
        <p role="alert" className="text-danger">
          {errorText(split.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
