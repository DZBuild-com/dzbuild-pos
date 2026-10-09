import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { Amount } from "@/components/amount";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Sheet } from "@/components/ui/sheet";
import { useTillKit } from "@/features/pos/till-kit";
import { useLogout } from "@/lib/ipc/auth";
import { useCompany } from "@/lib/ipc/company";
import { useDiningBoard, useRooms } from "@/lib/ipc/restaurant";
import type { DiningTable, Session, TableStatus } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { CounterTickets } from "./CounterTickets";
import { type OpenPlace, OpenTicketDialog } from "./OpenTicketDialog";
import { TableTile } from "./TableTile";

/** The floor's reading order: row by row, each from its first column. */
const byPlace = (a: DiningTable, b: DiningTable) => a.gridRow - b.gridRow || a.gridCol - b.gridCol;

/** Where an arrow key (or Home, End) goes from table `from` on the floor: left and right through
 * the tables in reading order, the screen's (in Arabic the left arrow is the next table), up and
 * down to the nearest table of the row above or below; `null` past the ends. */
function moveOnFloor(tables: DiningTable[], from: string, key: string, rtl: boolean): string | null {
  const order = [...tables].sort(byPlace);
  const index = order.findIndex((table) => table.id === from);
  const here = order[index];
  if (!here) return null;
  if (key === "Home") return order[0]?.id ?? null;
  if (key === "End") return order.at(-1)?.id ?? null;
  if (key === (rtl ? "ArrowLeft" : "ArrowRight")) return order[index + 1]?.id ?? null;
  if (key === (rtl ? "ArrowRight" : "ArrowLeft")) return order[index - 1]?.id ?? null;
  if (key !== "ArrowDown" && key !== "ArrowUp") return null;
  const down = key === "ArrowDown";
  const beyond = order.filter((table) =>
    down ? table.gridRow > here.gridRow : table.gridRow < here.gridRow,
  );
  const rows = beyond.map((table) => table.gridRow);
  const row = down ? Math.min(...rows) : Math.max(...rows);
  const distance = (table: DiningTable) => Math.abs(table.gridCol - here.gridCol);
  const nearest = beyond
    .filter((table) => table.gridRow === row)
    .reduce<DiningTable | null>(
      (best, table) => (best && distance(best) <= distance(table) ? best : table),
      null,
    );
  return nearest?.id ?? null;
}

const FLOOR_KEYS = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"];

/** `/pos/floor` (RST-001/002/003, UX-020): the till of a restaurant. Full window like the sale
 * screen: the rooms as tabs, each room a grid of its cells with its tables coloured by state, the
 * counter's tickets at the end side, and at the bottom the direct sale, the menu (Esc) and the
 * till's status. A tap opens the table's ticket (a chooser when it has several) or a new one with
 * its covers. Refreshed every 20 s and after every ticket write. */
export function FloorScreen({ session }: { session: Session }) {
  const { t, i18n } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const { BackupAlerts, DemoRibbon, LanBanner, StatusPills, useHotkeys } = useTillKit();
  const company = useCompany();
  const rooms = useRooms();
  const live = useMemo(
    () => (rooms.data ?? []).filter((r) => !r.archived).sort((a, b) => a.sortOrder - b.sortOrder),
    [rooms.data],
  );
  const [roomId, setRoomId] = useState<string | null>(null);
  const room = live.find((r) => r.id === roomId) ?? live[0] ?? null;
  // every table when the shop has no room (yet)
  const board = useDiningBoard(room?.id ?? null, !rooms.isPending);
  const [opening, setOpening] = useState<OpenPlace | null>(null);
  const [choosing, setChoosing] = useState<TableStatus | null>(null);
  const [menu, setMenu] = useState(false);
  // a new ticket prints in the shop's document language
  const docLang = company.data?.defaultDocLang ?? session.lang;
  const openTicket = (id: string) => void navigate(`/pos/ticket/${id}`);
  useHotkeys({ Escape: () => setMenu(true) }, !menu && opening === null && choosing === null);

  const tap = (status: TableStatus) => {
    const [only, ...more] = status.tickets;
    if (!only) setOpening({ kind: "table", table: status.table });
    else if (more.length === 0) openTicket(only.id);
    else setChoosing(status);
  };

  const tables = board.data ?? [];
  // RST-002 with the keyboard: the grid is one Tab stop (a roving focus), its first table when
  // the floor opens; the arrows move between the tables
  const tiles = useRef(new Map<string, HTMLButtonElement>());
  const [cursor, setCursor] = useState<string | null>(null);
  const first = tables.map((status) => status.table).sort(byPlace)[0]?.id ?? null;
  const stop = tables.some((status) => status.table.id === cursor) ? cursor : first;
  const tookFocus = useRef(false);
  useEffect(() => {
    if (tookFocus.current || first === null) return;
    tookFocus.current = true;
    // the floor takes the keyboard when nothing else has it (the till's screen it replaced)
    const focused = document.activeElement;
    if (focused === null || focused === document.body) tiles.current.get(first)?.focus();
  }, [first]);
  const onGridKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (stop === null || !FLOOR_KEYS.includes(event.key)) return;
    event.preventDefault();
    const rtl = i18n.dir() === "rtl";
    const next = moveOnFloor(
      tables.map((status) => status.table),
      stop,
      event.key,
      rtl,
    );
    if (next !== null) tiles.current.get(next)?.focus();
  };
  // a table left outside a smaller grid still shows
  const cols = Math.max(room?.gridCols ?? 1, ...tables.map((s) => s.table.gridCol + 1));
  const rows = Math.max(room?.gridRows ?? 1, ...tables.map((s) => s.table.gridRow + 1));

  return (
    <div className="flex h-dvh flex-col bg-bg text-[1.125rem] text-ink">
      <DemoRibbon />
      <BackupAlerts session={session} />
      <LanBanner />
      <header className="flex flex-wrap items-center gap-3 border-b border-line bg-surface px-4 py-2">
        <h1 className="text-2xl font-bold">{t("restaurant:floor.title")}</h1>
        {live.length > 0 ? (
          <div role="tablist" aria-label={t("restaurant:floor.rooms")} className="flex flex-wrap gap-2">
            {live.map((r) => (
              <button
                key={r.id}
                type="button"
                role="tab"
                aria-selected={r.id === room?.id}
                onClick={() => setRoomId(r.id)}
                className={cn(
                  "min-h-11 rounded-control px-4 font-semibold",
                  r.id === room?.id ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
                )}
              >
                {localizedName(r, lang)}
              </button>
            ))}
          </div>
        ) : null}
      </header>
      <div className="flex min-h-0 flex-1">
        <main role={live.length > 0 ? "tabpanel" : undefined} className="min-w-0 flex-1 overflow-auto p-4">
          {tables.length === 0 && board.isSuccess ? (
            <p className="text-ink-2">{t("restaurant:floor.noTables")}</p>
          ) : null}
          <div
            className="grid gap-3"
            onKeyDown={onGridKey}
            style={{
              gridTemplateColumns: `repeat(${cols}, minmax(96px, 1fr))`,
              gridTemplateRows: `repeat(${rows}, minmax(96px, auto))`,
            }}
          >
            {tables.map((status) => (
              <TableTile
                key={status.table.id}
                ref={(tile) => {
                  if (tile) tiles.current.set(status.table.id, tile);
                  else tiles.current.delete(status.table.id);
                }}
                tabIndex={status.table.id === stop ? 0 : -1}
                onFocus={() => setCursor(status.table.id)}
                status={status}
                onOpen={() => tap(status)}
                style={{ gridColumnStart: status.table.gridCol + 1, gridRowStart: status.table.gridRow + 1 }}
              />
            ))}
          </div>
        </main>
        <CounterTickets
          lang={docLang}
          onOpen={openTicket}
          onDelivery={() => setOpening({ kind: "delivery" })}
        />
      </div>
      <footer className="flex flex-wrap items-center gap-2 border-t border-line bg-surface px-4 py-2">
        <Button variant="outline" size="lg" onClick={() => void navigate("/pos/sale")}>
          {t("restaurant:floor.direct")}
        </Button>
        <Button
          size="lg"
          onClick={() => setMenu(true)}
          shortcut="Escape"
          shortcutLabel={t("common:keys.escape")}
        >
          {t("pos:actions.menu")}
        </Button>
        <div className="ms-auto">
          <StatusPills session={session} />
        </div>
      </footer>
      {opening ? (
        <OpenTicketDialog
          place={opening}
          lang={docLang}
          onClose={() => setOpening(null)}
          onOpened={(ticket) => openTicket(ticket.id)}
        />
      ) : null}
      {choosing ? (
        <TicketChooser
          status={choosing}
          onPick={openTicket}
          onNew={() => {
            setChoosing(null);
            setOpening({ kind: "table", table: choosing.table });
          }}
          onClose={() => setChoosing(null)}
        />
      ) : null}
      {menu ? <FloorMenu session={session} onClose={() => setMenu(false)} /> : null}
    </div>
  );
}

/** A table split between several tickets (RST-005): which one to open, or a new one. */
function TicketChooser({
  status,
  onPick,
  onNew,
  onClose,
}: {
  status: TableStatus;
  onPick: (ticketId: string) => void;
  onNew: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Dialog
      open
      onOpenChange={(next) => (next ? undefined : onClose())}
      title={t("restaurant:floor.choose")}
      description={t("restaurant:open.title", { code: status.table.code })}
      footer={
        <Button variant="outline" size="lg" onClick={onNew}>
          {t("restaurant:floor.newTicket")}
        </Button>
      }
    >
      <ul className="flex flex-col gap-2">
        {status.tickets.map((ticket) => (
          <li key={ticket.id}>
            <button
              type="button"
              onClick={() => onPick(ticket.id)}
              className="flex min-h-14 w-full items-center justify-between gap-3 rounded-card border border-line bg-surface-2 p-3 text-start hover:border-brand"
            >
              <span className="font-semibold">
                {t("restaurant:floor.covers", { count: ticket.covers })} · {ticket.waiterName}
              </span>
              <Amount value={ticket.totalTtc} className="font-bold" />
            </button>
          </li>
        ))}
      </ul>
    </Dialog>
  );
}

/** Esc on the floor: the screens of the till the F-keys of the sale screen do not cover. */
function FloorMenu({ session, onClose }: { session: Session; onClose: () => void }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { backOfficeHome } = useTillKit();
  const logout = useLogout();
  const go = (path: string) => {
    onClose();
    void navigate(path);
  };
  const manages =
    session.permissions.includes("settings.company") || session.permissions.includes("products.edit");
  const item = "justify-start";
  return (
    <Sheet open onOpenChange={(next) => (next ? undefined : onClose())} title={t("pos:menu.title")}>
      <div className="flex flex-col gap-3">
        <Button variant="outline" className={item} onClick={() => go("/sales")}>
          {t("pos:menu.history")}
        </Button>
        <Button variant="outline" className={item} onClick={() => go("/cash")}>
          {t("pos:menu.cash")}
        </Button>
        {manages ? (
          <Button variant="outline" className={item} onClick={() => go("/restaurant")}>
            {t("nav:restaurant")}
          </Button>
        ) : null}
        <Button variant="outline" className={item} onClick={() => go(backOfficeHome(session))}>
          {t("pos:menu.backOffice")}
        </Button>
        <Button variant="ghost" className={item} onClick={() => logout.mutate()}>
          {t("pos:menu.lock")}
        </Button>
      </div>
    </Sheet>
  );
}
