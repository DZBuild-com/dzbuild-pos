import { type KeyboardEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { useArchiveTable, useSaveTable, useTables } from "@/lib/ipc/restaurant";
import type { DiningTable, TableInput, TableShape } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { cn } from "@/lib/utils";

const SHAPES: TableShape[] = ["square", "round", "rect"];

interface Cell {
  col: number;
  row: number;
}

const inputOf = (table: DiningTable): TableInput => ({
  roomId: table.roomId,
  code: table.code,
  seats: table.seats,
  shape: table.shape,
  gridCol: table.gridCol,
  gridRow: table.gridRow,
});

/** The next free table code: T and the number after the highest one. */
function nextCode(tables: DiningTable[]): string {
  const numbers = tables.map((t) => Number(/(\d+)$/.exec(t.code)?.[1] ?? 0));
  return `T${Math.max(0, ...numbers) + 1}`;
}

/** RST-001: a room's plan. A table is dragged to a free cell with the pointer (the target cell
 * lights up; a taken cell is refused) or moved one cell with the arrow keys when focused (→ is
 * the end side: left in Arabic); a tap selects it for its code, seats and shape, or its archive.
 * "Ajouter une table" puts a new one on the first free cell. */
export function RoomEditor({
  roomId,
  gridCols,
  gridRows,
}: {
  roomId: string;
  gridCols: number;
  gridRows: number;
}) {
  const { t, i18n } = useTranslation();
  const errorText = useErrorText();
  const query = useTables(roomId);
  const tables = (query.data ?? []).filter((table) => !table.archived);
  const save = useSaveTable();
  const archive = useArchiveTable();
  const [dragging, setDragging] = useState<string | null>(null);
  const [target, setTarget] = useState<Cell | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const editing = tables.find((table) => table.id === selected) ?? null;

  // a drag released outside the plan ends there
  useEffect(() => {
    if (dragging === null) return undefined;
    const end = () => {
      setDragging(null);
      setTarget(null);
    };
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [dragging]);

  const taken = (col: number, row: number, except: string) =>
    tables.some((table) => table.id !== except && table.gridCol === col && table.gridRow === row);

  const move = (table: DiningTable, col: number, row: number) => {
    if (table.gridCol === col && table.gridRow === row) return;
    if (col < 0 || row < 0 || col >= gridCols || row >= gridRows) return;
    if (taken(col, row, table.id)) {
      setMessage(t("restaurant:editor.cellTaken"));
      return;
    }
    setMessage(null);
    save.mutate({ id: table.id, input: { ...inputOf(table), roomId, gridCol: col, gridRow: row } });
  };

  const drop = (col: number, row: number) => {
    if (!dragging) return;
    const table = tables.find((candidate) => candidate.id === dragging);
    setDragging(null);
    setTarget(null);
    if (!table) return;
    // released on its own cell: a tap, which selects the table (the press made the tables
    // transparent to the pointer, so the click lands on the plan, not on the table)
    if (table.gridCol === col && table.gridRow === row) setSelected(table.id);
    else move(table, col, row);
  };

  const onKey = (event: KeyboardEvent<HTMLButtonElement>, table: DiningTable) => {
    // logical arrows: → goes to the end side, which is the left in Arabic
    const end = i18n.dir() === "rtl" ? -1 : 1;
    const step: Record<string, Cell> = {
      ArrowRight: { col: end, row: 0 },
      ArrowLeft: { col: -end, row: 0 },
      ArrowUp: { col: 0, row: -1 },
      ArrowDown: { col: 0, row: 1 },
    };
    const delta = step[event.key];
    if (!delta) return;
    event.preventDefault();
    move(table, table.gridCol + delta.col, table.gridRow + delta.row);
  };

  const firstFree = (): Cell | null => {
    for (let row = 0; row < gridRows; row += 1) {
      for (let col = 0; col < gridCols; col += 1) if (!taken(col, row, "")) return { col, row };
    }
    return null;
  };
  const addTable = () => {
    const cell = firstFree();
    if (!cell) {
      setMessage(t("restaurant:editor.roomFull"));
      return;
    }
    save.mutate(
      {
        id: null,
        input: {
          roomId,
          code: nextCode(tables),
          seats: 4,
          shape: "square",
          gridCol: cell.col,
          gridRow: cell.row,
        },
      },
      { onSuccess: (table) => setSelected(table.id) },
    );
  };

  const cells: Cell[] = [];
  for (let row = 0; row < gridRows; row += 1)
    for (let col = 0; col < gridCols; col += 1) cells.push({ col, row });

  return (
    <div className="flex flex-col gap-4 lg:flex-row">
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={addTable} disabled={save.isPending}>
            {t("restaurant:editor.addTable")}
          </Button>
          <p className="text-sm text-ink-2">{t("restaurant:editor.help")}</p>
        </div>
        <div
          className="grid touch-none gap-2 select-none"
          style={{
            gridTemplateColumns: `repeat(${gridCols}, minmax(72px, 1fr))`,
            gridTemplateRows: `repeat(${gridRows}, 72px)`,
          }}
        >
          {cells.map(({ col, row }) => (
            <div
              key={`${col}-${row}`}
              data-testid={`cell-${col}-${row}`}
              style={{ gridColumnStart: col + 1, gridRowStart: row + 1 }}
              onPointerEnter={() => (dragging ? setTarget({ col, row }) : undefined)}
              onPointerMove={() => (dragging ? setTarget({ col, row }) : undefined)}
              onPointerUp={() => drop(col, row)}
              className={cn(
                "rounded-control border border-dashed border-line",
                target?.col === col && target.row === row
                  ? taken(col, row, dragging ?? "")
                    ? "bg-danger-soft"
                    : "bg-brand-soft"
                  : "bg-surface-2/50",
              )}
            />
          ))}
          {tables.map((table) => (
            <button
              key={table.id}
              type="button"
              aria-label={t("restaurant:editor.tableLabel", { code: table.code, seats: table.seats })}
              aria-pressed={selected === table.id}
              style={{ gridColumnStart: table.gridCol + 1, gridRowStart: table.gridRow + 1 }}
              onPointerDown={(event) => {
                // a touch screen captures the pointer on its target: let the cells under it see it
                if (event.currentTarget.hasPointerCapture(event.pointerId)) {
                  event.currentTarget.releasePointerCapture(event.pointerId);
                }
                setDragging(table.id);
                setMessage(null);
              }}
              onClick={() => setSelected(table.id)}
              onKeyDown={(event) => onKey(event, table)}
              className={cn(
                "flex cursor-grab flex-col items-center justify-center border-2 font-bold",
                table.shape === "round" ? "rounded-full" : "rounded-card",
                dragging === table.id ? "border-brand opacity-60" : "border-line bg-surface",
                selected === table.id ? "border-brand bg-brand-soft" : null,
                // while a table moves, the cells under the tables receive the pointer
                dragging !== null ? "pointer-events-none" : null,
              )}
            >
              <span>{table.code}</span>
              <span className="text-sm font-normal text-ink-2">
                {t("restaurant:floor.seats", { count: table.seats })}
              </span>
            </button>
          ))}
        </div>
        <p role="status" className="min-h-6 font-semibold text-danger">
          {message ?? ""}
        </p>
        {save.error ? (
          <p role="alert" className="text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
      </div>
      {editing ? (
        <TableForm
          key={editing.id}
          table={editing}
          busy={save.isPending || archive.isPending}
          onSave={(input) => save.mutate({ id: editing.id, input })}
          onArchive={() => archive.mutate(editing.id, { onSuccess: () => setSelected(null) })}
          error={archive.error}
        />
      ) : null}
    </div>
  );
}

function TableForm({
  table,
  busy,
  onSave,
  onArchive,
  error,
}: {
  table: DiningTable;
  busy: boolean;
  onSave: (input: TableInput) => void;
  onArchive: () => void;
  error: unknown;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const [code, setCode] = useState(table.code);
  const [seats, setSeats] = useState(String(table.seats));
  const [shape, setShape] = useState<TableShape>(table.shape);
  const count = Number(seats);
  const valid = code.trim() !== "" && Number.isInteger(count) && count >= 1 && count <= 40;
  return (
    <form
      noValidate
      className="flex w-full flex-col gap-3 rounded-card border border-line bg-surface p-4 lg:w-72"
      onSubmit={(event) => {
        event.preventDefault();
        if (valid) onSave({ ...inputOf(table), code: code.trim(), seats: count, shape });
      }}
    >
      <h3 className="text-lg font-bold">{t("restaurant:open.title", { code: table.code })}</h3>
      <Field id="table-code" label={t("restaurant:editor.code")}>
        <Input id="table-code" value={code} maxLength={8} onChange={(e) => setCode(e.target.value)} />
      </Field>
      <Field id="table-seats" label={t("restaurant:editor.seats")}>
        <NumericInput id="table-seats" kind="integer" value={seats} onValueChange={setSeats} />
      </Field>
      <Field id="table-shape" label={t("restaurant:editor.shape")}>
        <NativeSelect id="table-shape" value={shape} onChange={(e) => setShape(e.target.value as TableShape)}>
          {SHAPES.map((s) => (
            <option key={s} value={s}>
              {t(`restaurant:editor.shapes.${s}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary" disabled={!valid || busy}>
          {t("restaurant:editor.save")}
        </Button>
        <Button variant="danger" disabled={busy} onClick={onArchive}>
          {t("restaurant:editor.archive")}
        </Button>
      </div>
      {error ? (
        <p role="alert" className="text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </form>
  );
}
