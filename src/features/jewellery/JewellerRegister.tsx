import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Amount } from "@/components/amount";
import { type Columns, DataTable, columnHelper } from "@/components/data-table/data-table";
import { ExportMenu } from "@/components/data-table/export-menu";
import { LoadMore } from "@/components/data-table/list-notices";
import { NumericInput } from "@/components/numeric-input";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import { dayToMs, today } from "@/lib/dates";
import { formatAmountInput, formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { useCompany } from "@/lib/ipc/company";
import { useJewellerRegisterPages } from "@/lib/ipc/jewellery";
import { useTillLink } from "@/lib/ipc/lan";
import { type SettingKey, useSetSetting, useSetting } from "@/lib/ipc/settings";
import type { ExportTarget, JewellerRegisterEntry, JsonValue } from "@/lib/ipc/types";
import { formatQty, parseAmount } from "@/lib/numeric";
import { useErrorText } from "@/lib/use-error-text";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";

const col = columnHelper<JewellerRegisterEntry>();

/** The years a register can be read for: this year and the nine before it. */
const YEARS_BACK = 10;

interface Authorisation {
  number: string;
  date: string;
  authority: string;
}

/** `jewellery.authorisation` as stored (`{ number, date, authority }`), or `null`. */
function authorisationOf(value: JsonValue | undefined): Authorisation | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const { number, date, authority } = value as Record<string, unknown>;
  return typeof number === "string" && typeof date === "string" && typeof authority === "string"
    ? { number, date, authority }
    : null;
}

function sameAuthorisation(a: Authorisation | null, b: Authorisation | null): boolean {
  if (a === null || b === null) return a === b;
  return a.number === b.number && a.date === b.date && a.authority === b.authority;
}

/**
 * VRT-141 (CII art. 359, LF 2026 art. 62): the special client register of a year, every piece
 * sold in order with the buyer's identity, under the shop's trade authorisation. Exported and
 * printed as a PDF through M2's export menu (`ReportKind::JewellerRegister`). The register lives
 * on the main PC: a till reads it there and exports nothing.
 */
export function JewellerRegister({ year: firstYear }: { year: number }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const session = useSession();
  const till = useTillLink();
  const company = useCompany();
  const [year, setYear] = useState(firstYear);
  const pages = useJewellerRegisterPages(year);
  const authorisation = authorisationOf(useSetting("jewellery.authorisation").data);
  const entries = useMemo(() => pages.data?.pages.flat() ?? [], [pages.data]);
  const thisYear = Number(today().slice(0, 4));
  const years = Array.from({ length: YEARS_BACK }, (_, i) => thisYear - i);
  if (!years.includes(firstYear)) years.push(firstYear);
  const columns = useMemo<Columns<JewellerRegisterEntry>>(
    () =>
      col.columns([
        col.accessor("seq", {
          header: () => t("jewellery:register.columns.seq"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("businessDate", {
          header: () => t("jewellery:register.columns.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor("documentNumber", {
          header: () => t("jewellery:register.columns.ticket"),
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("nature", {
          header: () => t("jewellery:register.columns.nature"),
          cell: (c) => <span dir="auto">{c.getValue()}</span>,
        }),
        col.accessor("pieces", {
          header: () => t("jewellery:register.columns.pieces"),
          meta: { align: "end" },
        }),
        col.accessor("weightMg", {
          header: () => t("jewellery:register.columns.weight"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{formatQty(c.getValue(), lang)}</bdi>,
        }),
        col.accessor("titrePermille", {
          header: () => t("jewellery:register.columns.titre"),
          meta: { align: "end" },
          cell: (c) => <bdi dir="ltr">{c.getValue()}</bdi>,
        }),
        col.accessor("price", {
          header: () => t("jewellery:register.columns.price"),
          meta: { align: "end" },
          cell: (c) => <Amount value={c.getValue()} />,
        }),
        col.accessor("buyerName", {
          header: () => t("jewellery:register.columns.buyer"),
          cell: (c) => <span dir="auto">{c.getValue()}</span>,
        }),
        col.accessor("buyerIdNumber", {
          header: () => t("jewellery:register.columns.id"),
          cell: ({ row }) => (
            <span>
              {t(`jewellery:register.idKinds.${row.original.buyerIdKind}`)}{" "}
              <bdi dir="ltr">{row.original.buyerIdNumber}</bdi>
            </span>
          ),
        }),
        col.accessor("buyerAddress", {
          header: () => t("jewellery:register.columns.address"),
          enableSorting: false,
          cell: (c) => <span dir="auto">{c.getValue()}</span>,
        }),
      ]),
    [t, lang],
  );
  const target: ExportTarget = {
    kind: "report",
    request: { kind: "jeweller_register", from: `${year}-01-01`, to: `${year}-12-31`, lang },
  };
  const manages = session.data?.permissions.includes("settings.company") ?? false;
  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-1 rounded-card border border-line bg-surface p-4">
        <h2 className="text-2xl font-bold">{t("jewellery:register.title")}</h2>
        {company.data ? <p className="font-semibold">{localizedName(company.data, lang)}</p> : null}
        <p className={authorisation ? "text-ink" : "text-warn"}>
          {authorisation
            ? t("jewellery:register.authorisation", { ...authorisation })
            : t("jewellery:register.noAuthorisation")}
        </p>
      </header>
      <div className="flex flex-wrap items-end justify-between gap-3">
        <Field id="register-year" label={t("jewellery:register.year")}>
          <NativeSelect
            id="register-year"
            className="w-auto"
            value={year}
            onChange={(event) => setYear(Number(event.target.value))}
          >
            {years.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {till.isTill ? null : <ExportMenu target={target} />}
      </div>
      {till.isTill ? (
        <p role="note" className="rounded-control bg-info-soft p-3 text-info">
          {t("jewellery:register.tillNote")}
        </p>
      ) : null}
      {pages.error ? (
        <p role="alert" className="text-danger">
          {errorText(pages.error)}
        </p>
      ) : (
        <>
          <DataTable
            ariaLabel={t("jewellery:register.title")}
            data={entries}
            columns={columns}
            getRowId={(e) => `${e.year}-${e.seq}`}
            sortable={!pages.hasNextPage}
            empty={pages.isPending ? t("common:status.loading") : t("jewellery:register.empty")}
          />
          {pages.hasNextPage ? (
            <LoadMore onClick={() => void pages.fetchNextPage()} pending={pages.isFetchingNextPage} />
          ) : null}
        </>
      )}
      {manages ? <RegisterSettings authorisation={authorisation} readOnly={till.isTill} /> : null}
    </div>
  );
}

/** The trade authorisation printed on the register's header and the most a precious-metal
 * ticket may take in cash (VRT-144), M0 settings of `settings.company`. */
function RegisterSettings({
  authorisation,
  readOnly,
}: {
  authorisation: Authorisation | null;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const save = useSetSetting();
  const maxCash = useSetting("jewellery.max_cash").data;
  const storedMax = typeof maxCash === "number" ? maxCash : null;
  // Only what the user typed is state (an absent field, a `null` limit: untouched, the stored value
  // shows): the settings may arrive after the card mounts, and an untouched setting is not saved.
  const [typed, setTyped] = useState<Partial<Authorisation>>({});
  const [max, setMax] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [badMax, setBadMax] = useState(false);
  const shown = (field: keyof Authorisation) => typed[field] ?? authorisation?.[field] ?? "";
  const edit = (field: keyof Authorisation) => (value: string) =>
    setTyped((current) => ({ ...current, [field]: value }));
  const maxText = max ?? (storedMax === null ? "" : formatAmountInput(storedMax, lang));
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setError(null);
    const writes: Array<{ key: SettingKey; value: JsonValue }> = [];
    if (Object.keys(typed).length > 0) {
      const next = {
        number: shown("number").trim(),
        date: shown("date").trim(),
        authority: shown("authority").trim(),
      };
      const value = next.number === "" && next.date === "" && next.authority === "" ? null : next;
      if (!sameAuthorisation(value, authorisation)) writes.push({ key: "jewellery.authorisation", value });
    }
    if (max !== null) {
      const limit = max.trim() === "" ? null : parseAmount(max);
      if (max.trim() !== "" && (limit === null || limit <= 0)) {
        setBadMax(true);
        return;
      }
      if (limit !== storedMax) writes.push({ key: "jewellery.max_cash", value: limit });
    }
    try {
      for (const write of writes) await save.mutateAsync(write);
      setTyped({});
      setMax(null);
      toast.success(t("jewellery:register.saved"));
    } catch (e) {
      setError(e);
    }
  };
  return (
    <Card>
      <h2 className="text-xl font-bold">{t("jewellery:register.settings")}</h2>
      <HostOnlyNote />
      <form noValidate onSubmit={(event) => void submit(event)} className="flex flex-col gap-4">
        <fieldset disabled={readOnly} className="grid gap-4 sm:grid-cols-3">
          <Field id="register-number" label={t("jewellery:register.number")}>
            <Input
              id="register-number"
              dir="ltr"
              maxLength={40}
              value={shown("number")}
              onChange={(e) => edit("number")(e.target.value)}
            />
          </Field>
          <Field id="register-date" label={t("jewellery:register.date")}>
            <Input
              id="register-date"
              type="date"
              dir="ltr"
              value={shown("date")}
              onChange={(e) => edit("date")(e.target.value)}
            />
          </Field>
          <Field id="register-authority" label={t("jewellery:register.authority")}>
            <Input
              id="register-authority"
              dir="auto"
              maxLength={80}
              value={shown("authority")}
              onChange={(e) => edit("authority")(e.target.value)}
            />
          </Field>
          <Field
            id="register-max-cash"
            label={t("jewellery:register.maxCash")}
            hint={t("jewellery:register.maxCashHint")}
            error={badMax ? t("validation:amountFormat") : undefined}
          >
            <NumericInput
              id="register-max-cash"
              value={maxText}
              onValueChange={(v) => {
                setMax(v);
                setBadMax(false);
              }}
            />
          </Field>
        </fieldset>
        {error ? (
          <p role="alert" className="text-danger">
            {errorText(error)}
          </p>
        ) : null}
        {readOnly ? null : (
          <Button type="submit" variant="primary" className="self-start" disabled={save.isPending}>
            {t("jewellery:register.save")}
          </Button>
        )}
      </form>
    </Card>
  );
}
