import { Plus } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Route, Routes, useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { DataTable, columnHelper } from "@/components/data-table/data-table";
import { TableToolbar } from "@/components/data-table/table-toolbar";
import { DateRangePicker } from "@/components/date-range";
import { Button } from "@/components/ui/button";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { type DateRange, dayToMs, rangeOf, today } from "@/lib/dates";
import { formatDate, formatNumber } from "@/lib/format";
import { LANGS } from "@/lib/i18n";
import { useSession, useUserTiles } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import { useStockLocations } from "@/lib/ipc/stock";
import type { ExportTarget, Permission, Round, RoundStatus, StockLocation } from "@/lib/ipc/types";
import { useCreateRound, useCreateVan, useRounds } from "@/lib/ipc/wholesale";
import { localizedName } from "@/lib/names";
import { normalizeDigits } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { RoundPage, RoundStatusBadge } from "./RoundPage";

const STATUSES: readonly RoundStatus[] = ["planned", "out", "closed"];
/** The driver is a user of the shop, or someone else whose name is typed. */
const OTHER = "other";

/** `/rounds` (VRT-100, DOC-030): the delivery rounds of a period; a round at `/rounds/<id>`. */
export function RoundsScreen() {
  return (
    <Routes>
      <Route index element={<RoundsHome />} />
      <Route path=":id" element={<RoundRoute />} />
    </Routes>
  );
}

function RoundRoute() {
  const { id } = useParams();
  return <RoundPage key={id} id={id ?? ""} />;
}

const col = columnHelper<Round>();

function RoundsHome() {
  const { t } = useTranslation();
  const lang = useLang();
  const navigate = useNavigate();
  const errorText = useErrorText();
  const permissions = useSession().data?.permissions ?? [];
  const can = (p: Permission) => permissions.includes(p);
  const [range, setRange] = useState<DateRange>(() => rangeOf("this_month", today()));
  const [status, setStatus] = useState<RoundStatus | null>(null);
  const [search, setSearch] = useState("");
  const [creating, setCreating] = useState(false);
  const rounds = useRounds({ status, from: range.from, to: range.to });
  const text = normalizeDigits(search.trim()).toLocaleLowerCase();
  const rows = (rounds.data ?? []).filter(
    (r) =>
      text === "" ||
      [r.number, r.driverName, r.vehiclePlate ?? ""].some((v) => v.toLocaleLowerCase().includes(text)),
  );
  const exportTarget: ExportTarget | null = can("reports.sales")
    ? { kind: "report", request: { kind: "delivery_rounds", from: range.from, to: range.to } }
    : null;
  const columns = useMemo(
    () =>
      col.columns([
        col.accessor("number", {
          header: () => t("wholesale:rounds.number"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("businessDate", {
          header: () => t("wholesale:rounds.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor("driverName", { header: () => t("wholesale:rounds.driver") }),
        col.accessor((r) => r.vehiclePlate ?? "", {
          id: "plate",
          header: () => t("wholesale:rounds.plate"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor((r) => r.stops.length, {
          id: "stops",
          header: () => t("wholesale:rounds.stops"),
          meta: { align: "end" },
          cell: (c) => formatNumber(c.getValue(), lang),
        }),
        col.accessor("totalDue", {
          header: () => t("wholesale:rounds.totalDue"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("totalCollected", {
          header: () => t("wholesale:rounds.totalCollected"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("status", {
          header: () => t("wholesale:rounds.state"),
          cell: (c) => <RoundStatusBadge status={c.getValue()} />,
        }),
      ]),
    [t, lang],
  );

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-3xl font-bold">{t("wholesale:rounds.title")}</h1>
        {can("documents.delivery") ? (
          <Button variant="primary" onClick={() => setCreating(true)}>
            <Plus aria-hidden />
            {t("wholesale:rounds.new")}
          </Button>
        ) : null}
      </div>
      <TableToolbar
        search={search}
        onSearch={setSearch}
        searchHint={t("wholesale:rounds.searchHint")}
        exportTarget={exportTarget}
        filters={
          <>
            <DateRangePicker value={range} onChange={setRange} />
            <Field id="rounds-status" label={t("wholesale:rounds.state")}>
              <NativeSelect
                id="rounds-status"
                className="w-auto"
                value={status ?? ""}
                onChange={(event) => setStatus(STATUSES.find((s) => s === event.target.value) ?? null)}
              >
                <option value="">{t("wholesale:rounds.allStates")}</option>
                {STATUSES.map((s) => (
                  <option key={s} value={s}>
                    {t(`wholesale:rounds.status.${s}`)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </>
        }
      />
      {rounds.error ? (
        <p role="alert" className="text-danger">
          {errorText(rounds.error)}
        </p>
      ) : (
        <DataTable
          ariaLabel={t("wholesale:rounds.label")}
          data={rows}
          columns={columns}
          getRowId={(r) => r.id}
          onRowClick={(r) => void navigate(r.id)}
          empty={rounds.isPending ? t("common:status.loading") : t("wholesale:rounds.empty")}
        />
      )}
      {creating ? (
        <NewRoundDialog
          onClose={() => setCreating(false)}
          onCreated={(round) => {
            setCreating(false);
            void navigate(round.id);
          }}
        />
      ) : null}
    </div>
  );
}

/** A van a round can take: a storage location of its own, never the shop's default one. */
function isVan(location: StockLocation): boolean {
  return !location.archived && !location.isDefault && location.kind === "storage";
}

/** "Nouvelle tournée": the driver (a user, or a name), his plate, the van, the day. */
function NewRoundDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (round: Round) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const users = useUserTiles().data ?? [];
  const vans = (useStockLocations().data ?? []).filter(isVan);
  const create = useCreateRound();
  const [driver, setDriver] = useState<string>("");
  const [driverName, setDriverName] = useState("");
  const [plate, setPlate] = useState("");
  const [van, setVan] = useState("");
  const [day, setDay] = useState(today());
  const [newVan, setNewVan] = useState(false);
  const [missing, setMissing] = useState(false);
  const user = users.find((u) => u.id === driver);
  const name = driver === OTHER ? driverName.trim() : (user?.displayName ?? "");
  const refused = toAppError(create.error);
  const fieldError = (field: string) =>
    create.error && refused.field === field ? errorText(create.error) : undefined;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (name === "") {
      setMissing(true);
      return;
    }
    create.mutate(
      {
        businessDate: day === "" ? null : day,
        driverName: name,
        driverUserId: user?.id ?? null,
        vehiclePlate: plate.trim() === "" ? null : plate.trim(),
        vanLocationId: van === "" ? null : van,
        note: null,
      },
      {
        onSuccess: (round) => {
          toast.success(t("wholesale:rounds.created", { number: round.number }));
          onCreated(round);
        },
      },
    );
  };

  if (newVan) {
    return (
      <NewVanDialog
        onClose={() => setNewVan(false)}
        onCreated={(location) => {
          setVan(location.id);
          setNewVan(false);
        }}
      />
    );
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.new")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="new-round" variant="primary" disabled={create.isPending}>
            {t("wholesale:rounds.create")}
          </Button>
        </>
      }
    >
      <form id="new-round" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field
          id="round-driver"
          label={t("wholesale:rounds.driver")}
          error={missing && driver !== OTHER ? t("validation:required") : fieldError("driverUserId")}
        >
          <NativeSelect
            id="round-driver"
            value={driver}
            onChange={(event) => {
              setDriver(event.target.value);
              setMissing(false);
            }}
          >
            <option value="" disabled>
              {t("wholesale:rounds.driver")}
            </option>
            {users.map((u) => (
              <option key={u.id} value={u.id}>
                {u.displayName}
              </option>
            ))}
            <option value={OTHER}>{t("wholesale:rounds.driverOther")}</option>
          </NativeSelect>
        </Field>
        {driver === OTHER ? (
          <Field
            id="round-driver-name"
            label={t("wholesale:rounds.driverName")}
            error={missing ? t("validation:required") : fieldError("driverName")}
          >
            <Input
              id="round-driver-name"
              dir="auto"
              autoFocus
              maxLength={80}
              value={driverName}
              onChange={(event) => {
                setDriverName(event.target.value);
                setMissing(false);
              }}
            />
          </Field>
        ) : null}
        <Field
          id="round-plate"
          label={t("wholesale:rounds.plate")}
          optional={t("wholesale:rounds.optional")}
          error={fieldError("vehiclePlate")}
        >
          <Input
            id="round-plate"
            dir="ltr"
            maxLength={20}
            value={plate}
            onChange={(e) => setPlate(e.target.value)}
          />
        </Field>
        <div className="flex flex-wrap items-end gap-3">
          <div className="min-w-60 flex-1">
            <Field
              id="round-van"
              label={t("wholesale:rounds.van")}
              optional={t("wholesale:rounds.optional")}
              error={fieldError("vanLocationId") ?? fieldError("locationId")}
            >
              <NativeSelect id="round-van" value={van} onChange={(event) => setVan(event.target.value)}>
                <option value="">{t("wholesale:rounds.noVan")}</option>
                {vans.map((v) => (
                  <option key={v.id} value={v.id}>
                    {localizedName(v, lang)}
                  </option>
                ))}
              </NativeSelect>
            </Field>
          </div>
          <Button variant="outline" onClick={() => setNewVan(true)}>
            <Plus aria-hidden />
            {t("wholesale:rounds.vanNew")}
          </Button>
        </div>
        <Field id="round-date" label={t("wholesale:rounds.date")} error={fieldError("businessDate")}>
          <Input id="round-date" type="date" dir="ltr" value={day} onChange={(e) => setDay(e.target.value)} />
        </Field>
        {create.error && !refused.field ? (
          <p role="alert" className="text-danger">
            {errorText(create.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

/** A delivery van: a stock location of its own, named in the three languages. */
function NewVanDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (location: StockLocation) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const create = useCreateVan();
  const [code, setCode] = useState("");
  const [names, setNames] = useState({ ar: "", fr: "", en: "" });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    create.mutate(
      {
        code: code.trim(),
        names: { nameAr: names.ar.trim(), nameFr: names.fr.trim(), nameEn: names.en.trim() },
      },
      {
        onSuccess: (location) => {
          toast.success(t("wholesale:rounds.vanCreated", { name: localizedName(location, lang) }));
          onCreated(location);
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.vanNew")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="new-van" variant="primary" disabled={create.isPending}>
            {t("wholesale:rounds.vanCreate")}
          </Button>
        </>
      }
    >
      <form id="new-van" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <Field id="van-code" label={t("wholesale:rounds.vanCode")}>
          <Input
            id="van-code"
            dir="ltr"
            autoFocus
            maxLength={48}
            value={code}
            onChange={(e) => setCode(e.target.value)}
          />
        </Field>
        {LANGS.map((l) => (
          <Field
            key={l}
            id={`van-name-${l}`}
            label={t("wholesale:rounds.vanName", { lang: t(`common:languages.${l}`) })}
          >
            <Input
              id={`van-name-${l}`}
              lang={l}
              dir={l === "ar" ? "rtl" : "ltr"}
              maxLength={120}
              value={names[l]}
              onChange={(e) => setNames((all) => ({ ...all, [l]: e.target.value }))}
            />
          </Field>
        ))}
        {create.error ? (
          <p role="alert" className="text-danger">
            {errorText(create.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
