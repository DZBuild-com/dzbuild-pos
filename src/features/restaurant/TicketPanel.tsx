import { Check, Gift, Minus, Pencil, Plus, Trash2, Undo2 } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useEditLine } from "@/lib/ipc/restaurant";
import type { Ticket, TicketLine } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";

/** The courses a line is served in (RST-012: a label on the kitchen ticket; firing by course is
 * V1), as the backend numbers them. */
export const COURSES = [
  { value: 1, key: "starter" },
  { value: 2, key: "main" },
  { value: 3, key: "dessert" },
  { value: 4, key: "drink" },
] as const;

/** The seats of a ticket: 0 (shared), then 1 to its covers (at least 1). */
export function seatsOf(ticket: Ticket): number[] {
  return [0, ...Array.from({ length: Math.max(1, ticket.covers) }, (_, i) => i + 1)];
}

export interface PanelActions {
  onSelect: (line: TicketLine) => void;
  onQty: (line: TicketLine, delta: 1 | -1) => void;
  onRemove: (line: TicketLine) => void;
  onEdit: (line: TicketLine) => void;
  onVoid: (line: TicketLine) => void;
  onComp: (line: TicketLine) => void;
}

/** A choice of chips (the seat and the course new lines go to). */
function Chips<T extends string | number | null>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      <span className="me-1 text-sm font-semibold text-ink-2">{label}</span>
      {options.map((option) => (
        <button
          key={String(option.value)}
          type="button"
          role="radio"
          aria-checked={value === option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            "min-h-11 min-w-11 rounded-full border px-3 text-sm font-semibold",
            value === option.value ? "border-brand bg-brand-soft" : "border-line bg-surface",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** Where the next lines go: the seat (0 = shared) and the course. */
export function PlacePickers({
  ticket,
  seat,
  course,
  onSeat,
  onCourse,
}: {
  ticket: Ticket;
  seat: number;
  course: number | null;
  onSeat: (seat: number) => void;
  onCourse: (course: number | null) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-2">
      <Chips
        label={t("restaurant:ticket.seat")}
        value={seat}
        onChange={onSeat}
        options={seatsOf(ticket).map((s) => ({
          value: s,
          label: s === 0 ? t("restaurant:ticket.shared") : String(s),
        }))}
      />
      <Chips<number | null>
        label={t("restaurant:ticket.course")}
        value={course}
        onChange={onCourse}
        options={[
          { value: null, label: t("restaurant:ticket.courses.none") },
          ...COURSES.map((c) => ({ value: c.value, label: t(`restaurant:ticket.courses.${c.key}`) })),
        ]}
      />
    </div>
  );
}

/** The table ticket at the end side of the ticket screen (440 px, RST-002): its lines grouped by
 * seat and course, the options under their dish; a sent line shows its check mark and what the
 * kitchen received, an unsent one is edited in place (quantity, note, seat, course, removal); a
 * sent line is voided with a reason. The total is the backend's. */
export function TicketPanel({
  ticket,
  selected,
  actions,
  pickers,
  footer,
}: {
  ticket: Ticket;
  selected: string | null;
  actions: PanelActions;
  pickers: ReactNode;
  footer: ReactNode;
}) {
  const { t } = useTranslation();
  const mains = ticket.lines.filter((l) => l.parentLineId === null);
  const optionsOf = (line: TicketLine) => ticket.lines.filter((l) => l.parentLineId === line.id);
  const seats = [...new Set(mains.map((l) => l.seat ?? 0))].sort((a, b) => a - b);
  const order = (l: TicketLine) => [l.course ?? 99, l.position] as const;
  return (
    <aside className="flex w-[400px] shrink-0 flex-col gap-3 border-s border-line bg-surface p-4 xl:w-[440px]">
      {pickers}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {mains.length === 0 ? <p className="p-3 text-ink-2">{t("restaurant:ticket.empty")}</p> : null}
        {seats.map((s) => (
          <section key={s} aria-labelledby={`seat-${s}`} className="mb-3">
            <h3 id={`seat-${s}`} className="mb-1 text-sm font-bold text-ink-2">
              {s === 0 ? t("restaurant:ticket.shared") : t("restaurant:ticket.seatNumber", { number: s })}
            </h3>
            <ul className="flex flex-col gap-1">
              {mains
                .filter((l) => (l.seat ?? 0) === s)
                .sort((a, b) => {
                  const [ca, pa] = order(a);
                  const [cb, pb] = order(b);
                  return ca - cb || pa - pb;
                })
                .map((line) => (
                  <Line
                    key={line.id}
                    line={line}
                    options={optionsOf(line)}
                    selected={selected === line.id}
                    actions={actions}
                  />
                ))}
            </ul>
          </section>
        ))}
      </div>
      <div className="flex items-center justify-between rounded-card bg-brand px-4 py-3 text-brand-ink">
        <span className="text-lg font-semibold">{t("restaurant:ticket.total")}</span>
        <Amount value={ticket.totalTtc} className="text-3xl leading-none font-bold" />
      </div>
      {footer}
    </aside>
  );
}

function Line({
  line,
  options,
  selected,
  actions,
}: {
  line: TicketLine;
  options: TicketLine[];
  selected: boolean;
  actions: PanelActions;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const sent = line.sentQty > 0;
  const course = COURSES.find((c) => c.value === line.course);
  const icon = "min-h-11 min-w-11 px-2";
  return (
    <li
      className={cn(
        "flex flex-col gap-1 rounded-control border p-2",
        selected ? "border-brand bg-brand-soft" : "border-transparent",
      )}
    >
      <button
        type="button"
        aria-pressed={selected}
        onClick={() => actions.onSelect(line)}
        className="flex w-full items-start gap-2 text-start"
      >
        <bdi dir="ltr" className="min-w-10 font-bold tabular-nums">
          {formatQty(line.qty, lang)}
        </bdi>
        <span className="flex flex-1 flex-col">
          <span className="font-semibold">{line.name}</span>
          {options.map((option) => (
            <span key={option.id} className="text-sm text-ink-2">
              + {option.name}
            </span>
          ))}
          {line.cartLine.note ? (
            <span className="text-sm text-ink-2 italic">{line.cartLine.note}</span>
          ) : null}
          <span className="flex flex-wrap gap-2 text-sm text-ink-2">
            {course ? <span>{t(`restaurant:ticket.courses.${course.key}`)}</span> : null}
            {line.compReason ? (
              <span className="font-semibold text-ok">{t(`restaurant:comp.reasons.${line.compReason}`)}</span>
            ) : null}
          </span>
        </span>
        {sent ? (
          <span className="flex items-center gap-1 text-sm font-semibold text-ok">
            <Check aria-hidden className="size-4" />
            {t("restaurant:ticket.sentQty", { qty: formatQty(line.sentQty, lang) })}
          </span>
        ) : (
          <span className="rounded-full bg-warn-soft px-2 text-sm font-semibold text-warn">
            {t("restaurant:ticket.unsent")}
          </span>
        )}
      </button>
      <div className="flex flex-wrap justify-end gap-1">
        {sent ? null : (
          <>
            <Button
              variant="ghost"
              className={icon}
              aria-label={t("restaurant:ticket.less", { name: line.name })}
              disabled={line.qty <= 1_000}
              onClick={() => actions.onQty(line, -1)}
            >
              <Minus aria-hidden />
            </Button>
            <Button
              variant="ghost"
              className={icon}
              aria-label={t("restaurant:ticket.more", { name: line.name })}
              onClick={() => actions.onQty(line, 1)}
            >
              <Plus aria-hidden />
            </Button>
            <Button
              variant="ghost"
              className={icon}
              aria-label={t("restaurant:ticket.editLine", { name: line.name })}
              onClick={() => actions.onEdit(line)}
            >
              <Pencil aria-hidden />
            </Button>
            <Button
              variant="ghost"
              className={icon}
              aria-label={t("restaurant:ticket.remove", { name: line.name })}
              onClick={() => actions.onRemove(line)}
            >
              <Trash2 aria-hidden />
            </Button>
          </>
        )}
        {line.compReason ? null : (
          <Button
            variant="ghost"
            className={icon}
            aria-label={t("restaurant:ticket.compLine", { name: line.name })}
            onClick={() => actions.onComp(line)}
          >
            <Gift aria-hidden />
          </Button>
        )}
        {sent ? (
          <Button
            variant="ghost"
            className={icon}
            aria-label={t("restaurant:ticket.voidLine", { name: line.name })}
            onClick={() => actions.onVoid(line)}
          >
            <Undo2 aria-hidden className="rtl:-scale-x-100" />
          </Button>
        ) : null}
      </div>
    </li>
  );
}

/** An unsent line's note (what the kitchen reads), seat and course. */
export function LineEditDialog({
  ticket,
  line,
  onClose,
}: {
  ticket: Ticket;
  line: TicketLine;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const edit = useEditLine();
  const [note, setNote] = useState(line.cartLine.note ?? "");
  const [seat, setSeat] = useState(line.seat ?? 0);
  const [course, setCourse] = useState<number | null>(line.course);
  const save = () =>
    edit.mutate(
      {
        lineId: line.id,
        edit: { seat: seat === 0 ? null : seat, course, note: note.trim() === "" ? null : note.trim() },
      },
      { onSuccess: onClose },
    );
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("restaurant:ticket.editTitle")}
      description={line.name}
      footer={
        <>
          <Button variant="outline" size="lg" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" size="lg" disabled={edit.isPending} onClick={save}>
            {t("restaurant:ticket.saveLine")}
          </Button>
        </>
      }
    >
      <Field id="line-note" label={t("restaurant:ticket.note")}>
        <Input id="line-note" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
      </Field>
      <PlacePickers ticket={ticket} seat={seat} course={course} onSeat={setSeat} onCourse={setCourse} />
      {edit.error ? (
        <p role="alert" className="text-danger">
          {errorText(edit.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
