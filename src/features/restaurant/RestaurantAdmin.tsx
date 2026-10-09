import { LayoutGrid } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";
import { EmptyState } from "@/components/empty-state";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useArchiveRoom, useRooms, useSaveRoom } from "@/lib/ipc/restaurant";
import type { Permission, Room, RoomInput, Session } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { DailyMenuEditor } from "./DailyMenuEditor";
import { ModifiersEditor } from "./ModifiersEditor";
import { RestaurantSettings } from "./RestaurantSettings";
import { RoomEditor } from "./RoomEditor";
import { NameFields, StationsEditor } from "./StationsEditor";

type Tab = "rooms" | "stations" | "modifiers" | "menu" | "settings";

/** Who edits each tab: the floor and the stations are the shop's settings, the options and the
 * daily menu the catalogue's (the backend checks them again). */
const TAB_PERMISSIONS: Record<Tab, Permission> = {
  rooms: "settings.company",
  stations: "settings.company",
  modifiers: "products.edit",
  menu: "products.edit",
  // final review: the kitchen tickets' language, the counter's tickets, the covers
  settings: "settings.company",
};

/** `/restaurant` (RST-001, RST-006, SAL-024): the dining rooms and their plans, the preparation
 * stations, the option groups and the daily menu. A waiter without these rights finds the way to
 * the floor. */
export function RestaurantAdmin({ session }: { session: Session }) {
  const { t } = useTranslation();
  const [params, setParams] = useSearchParams();
  const tabs = (Object.keys(TAB_PERMISSIONS) as Tab[]).filter((k) =>
    session.permissions.includes(TAB_PERMISSIONS[k]),
  );
  const requested = params.get("tab");
  const tab = tabs.find((k) => k === requested) ?? tabs[0];
  const floor = session.permissions.includes("sales.create") ? (
    <Button asChild variant="secondary">
      <Link to="/pos/floor">{t("restaurant:admin.openFloor")}</Link>
    </Button>
  ) : null;
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("nav:restaurant")}</h1>
        {tab ? floor : null}
      </div>
      {tab ? (
        <>
          <div role="tablist" className="flex flex-wrap gap-2 border-b border-line pb-2">
            {tabs.map((k) => (
              <button
                key={k}
                type="button"
                role="tab"
                id={`restaurant-tab-${k}`}
                aria-selected={tab === k}
                aria-controls={`restaurant-panel-${k}`}
                onClick={() => setParams({ tab: k }, { replace: true })}
                className={cn(
                  "min-h-11 rounded-control px-4 font-semibold",
                  tab === k ? "bg-brand-soft" : "text-ink-2 hover:bg-surface-2",
                )}
              >
                {t(`restaurant:admin.${k}`)}
              </button>
            ))}
          </div>
          <div role="tabpanel" id={`restaurant-panel-${tab}`} aria-labelledby={`restaurant-tab-${tab}`}>
            {tab === "rooms" ? <RoomsTab /> : null}
            {tab === "stations" ? <StationsEditor /> : null}
            {tab === "modifiers" ? <ModifiersEditor /> : null}
            {tab === "menu" ? <DailyMenuEditor /> : null}
            {tab === "settings" ? <RestaurantSettings /> : null}
          </div>
        </>
      ) : (
        <EmptyState
          icon={LayoutGrid}
          title={t("restaurant:admin.waiterTitle")}
          body={t("restaurant:admin.waiterBody")}
          action={floor}
        />
      )}
    </div>
  );
}

const NEW_ROOM: RoomInput = {
  code: "",
  nameAr: "",
  nameFr: "",
  nameEn: "",
  gridCols: 4,
  gridRows: 3,
  sortOrder: 0,
};

/** The rooms (terrace, first floor…) with their grid, then the selected room's plan. */
function RoomsTab() {
  const { t } = useTranslation();
  const lang = useLang();
  const rooms = useRooms();
  const list = (rooms.data ?? []).filter((r) => !r.archived).sort((a, b) => a.sortOrder - b.sortOrder);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const room = list.find((r) => r.id === selectedId) ?? list[0] ?? null;
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        {list.map((r) => (
          <button
            key={r.id}
            type="button"
            aria-pressed={!adding && r.id === room?.id}
            onClick={() => {
              setAdding(false);
              setSelectedId(r.id);
            }}
            className={cn(
              "min-h-11 rounded-control border px-4 font-semibold",
              !adding && r.id === room?.id ? "border-brand bg-brand-soft" : "border-line bg-surface",
            )}
          >
            {localizedName(r, lang)}
          </button>
        ))}
        <Button variant="outline" onClick={() => setAdding(true)}>
          {t("restaurant:editor.addRoom")}
        </Button>
      </div>
      {adding ? (
        <RoomForm
          key="new"
          room={null}
          onSaved={(saved) => {
            setAdding(false);
            setSelectedId(saved.id);
          }}
          onCancel={() => setAdding(false)}
        />
      ) : room ? (
        <>
          <RoomForm key={room.id} room={room} onSaved={() => undefined} onCancel={() => undefined} />
          <RoomEditor roomId={room.id} gridCols={room.gridCols} gridRows={room.gridRows} />
        </>
      ) : rooms.isSuccess ? (
        <p className="text-ink-2">{t("restaurant:editor.noRooms")}</p>
      ) : null}
    </div>
  );
}

function RoomForm({
  room,
  onSaved,
  onCancel,
}: {
  room: Room | null;
  onSaved: (room: Room) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSaveRoom();
  const archive = useArchiveRoom();
  const [input, setInput] = useState<RoomInput>(
    room
      ? {
          code: room.code,
          nameAr: room.nameAr,
          nameFr: room.nameFr,
          nameEn: room.nameEn,
          gridCols: room.gridCols,
          gridRows: room.gridRows,
          sortOrder: room.sortOrder,
        }
      : NEW_ROOM,
  );
  const [cols, setCols] = useState(String(input.gridCols));
  const [rows, setRows] = useState(String(input.gridRows));
  const gridCols = Number(cols);
  const gridRows = Number(rows);
  const inRange = (n: number) => Number.isInteger(n) && n >= 1 && n <= 12;
  const valid =
    input.code.trim() !== "" &&
    (input.nameFr.trim() !== "" || input.nameAr.trim() !== "") &&
    inRange(gridCols) &&
    inRange(gridRows);
  return (
    <form
      noValidate
      className="flex flex-col gap-3 rounded-card border border-line bg-surface p-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (valid) {
          save.mutate(
            { id: room?.id ?? null, input: { ...input, code: input.code.trim(), gridCols, gridRows } },
            { onSuccess: onSaved },
          );
        }
      }}
    >
      <div className="grid gap-3 sm:grid-cols-3">
        <Field id="room-code" label={t("restaurant:editor.code")}>
          <Input
            id="room-code"
            dir="ltr"
            value={input.code}
            maxLength={12}
            onChange={(e) => setInput({ ...input, code: e.target.value })}
          />
        </Field>
        <Field id="room-cols" label={t("restaurant:editor.gridCols")}>
          <NumericInput id="room-cols" kind="integer" value={cols} onValueChange={setCols} />
        </Field>
        <Field id="room-rows" label={t("restaurant:editor.gridRows")}>
          <NumericInput id="room-rows" kind="integer" value={rows} onValueChange={setRows} />
        </Field>
      </div>
      <NameFields id="room-name" value={input} onChange={(names) => setInput({ ...input, ...names })} />
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary" disabled={!valid || save.isPending}>
          {t("restaurant:editor.save")}
        </Button>
        {room ? (
          <Button variant="danger" disabled={archive.isPending} onClick={() => archive.mutate(room.id)}>
            {t("restaurant:editor.archive")}
          </Button>
        ) : (
          <Button variant="ghost" onClick={onCancel}>
            {t("common:actions.cancel")}
          </Button>
        )}
      </div>
      {save.error || archive.error ? (
        <p role="alert" className="text-danger">
          {errorText(save.error ?? archive.error)}
        </p>
      ) : null}
    </form>
  );
}
