import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { LoadMore } from "@/components/data-table/list-notices";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatDate, formatNumber } from "@/lib/format";
import { useArchive } from "@/lib/ipc/billing";
import type { ArchiveRow, DocState, Round, RoundFilter } from "@/lib/ipc/types";
import { useRounds } from "@/lib/ipc/wholesale";
import { normalizeDigits } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** Every open round, whatever its day (the rounds list takes a period). */
const ALL_DAYS = { from: "2000-01-01", to: "9999-12-31" } as const;
const OPEN_PLANNED: RoundFilter = { status: "planned", ...ALL_DAYS };
const OPEN_OUT: RoundFilter = { status: "out", ...ALL_DAYS };

/** What the customer still owes on a validated delivery note or invoice, from M3's document state:
 * a delivery note waiting for its invoice owes its total, an unpaid or partly credited invoice
 * what is left; `null` when nothing is owed (paid, invoiced, credited). */
function openAmount(row: ArchiveRow): number | null {
  const state: DocState = row.state;
  switch (state.state) {
    case "to_invoice":
      return row.totalDue;
    case "unpaid":
    case "partly_credited":
      return state.remaining > 0 ? state.remaining : null;
    default:
      return null;
  }
}

/**
 * The documents a planned round can carry (VRT-100): M3's archive of validated delivery notes and
 * invoices, kept to those whose customer still owes on them and that ride no other open round;
 * ticked ones join the round after its stops. The backend checks each again.
 */
export function StopPicker({
  round,
  pending,
  onPick,
  onClose,
}: {
  round: Round;
  pending: boolean;
  onPick: (documentIds: string[]) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [search, setSearch] = useState("");
  const text = useDebounced(normalizeDigits(search.trim()), 200);
  const [chosen, setChosen] = useState<string[]>([]);
  const archive = useArchive({
    kinds: ["delivery_note", "invoice"],
    status: "validated",
    text: text || null,
  });
  const planned = useRounds(OPEN_PLANNED);
  const out = useRounds(OPEN_OUT);
  // this round's own stops, and every stop of the other open rounds
  const riding = new Set(
    [round, ...(planned.data ?? []), ...(out.data ?? [])].flatMap((r) => r.stops.map((s) => s.documentId)),
  );
  const rows = (archive.data?.pages ?? [])
    .flatMap((page) => page.rows)
    .filter((row) => openAmount(row) !== null && !riding.has(row.id));
  const toggle = (id: string, on: boolean) =>
    setChosen((all) => (on ? [...all.filter((c) => c !== id), id] : all.filter((c) => c !== id)));

  return (
    <Dialog
      open
      size="xl"
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("wholesale:rounds.pick")}
      description={t("wholesale:rounds.pickHint")}
      footer={
        <>
          <span className="me-auto text-ink-2" role="status">
            {t("wholesale:rounds.picked", { count: formatNumber(chosen.length, lang) })}
          </span>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={pending || chosen.length === 0}
            onClick={() => onPick(rows.filter((r) => chosen.includes(r.id)).map((r) => r.id))}
          >
            {t("wholesale:rounds.pickSubmit")}
          </Button>
        </>
      }
    >
      <Field id="round-pick-search" label={t("tables:search")}>
        <Input
          id="round-pick-search"
          type="search"
          dir="auto"
          placeholder={t("wholesale:rounds.pickSearch")}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </Field>
      {archive.error ? (
        <p role="alert" className="text-danger">
          {errorText(archive.error)}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-card border border-line">
          <table aria-label={t("wholesale:rounds.pick")} className="w-full border-collapse">
            <thead className="bg-surface-2">
              <tr className="border-b border-line text-sm text-ink-2">
                <th scope="col" className="w-12 px-3 py-2">
                  <span className="sr-only">{t("wholesale:rounds.pickSubmit")}</span>
                </th>
                <th scope="col" className="px-3 py-2 text-start font-semibold">
                  {t("wholesale:rounds.document")}
                </th>
                <th scope="col" className="px-3 py-2 text-start font-semibold">
                  {t("wholesale:rounds.customer")}
                </th>
                <th scope="col" className="px-3 py-2 text-start font-semibold">
                  {t("wholesale:rounds.date")}
                </th>
                <th scope="col" className="px-3 py-2 text-end font-semibold">
                  {t("wholesale:rounds.amountDue")}
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={5} className="px-3 py-6 text-center text-ink-2">
                    {archive.isPending ? t("common:status.loading") : t("wholesale:rounds.pickEmpty")}
                  </td>
                </tr>
              ) : (
                rows.map((row) => {
                  const number = row.number ?? "";
                  return (
                    <tr key={row.id} className="border-b border-line even:bg-surface-2">
                      <td className="px-3 py-2">
                        <Checkbox
                          aria-label={number}
                          checked={chosen.includes(row.id)}
                          onCheckedChange={(on) => toggle(row.id, on === true)}
                        />
                      </td>
                      <td className="px-3 py-2">
                        <bdi dir="ltr">{number}</bdi>{" "}
                        <span className="text-sm text-ink-2">{t(`documents:types.${row.docType}`)}</span>
                      </td>
                      <td className="px-3 py-2" dir="auto">
                        {row.partyName ?? ""}
                      </td>
                      <td className="px-3 py-2">
                        <bdi dir="ltr">{formatDate(dayToMs(row.businessDate), lang)}</bdi>
                      </td>
                      <td className="px-3 py-2 text-end">
                        <Amount value={openAmount(row) ?? 0} />
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      )}
      {archive.hasNextPage ? (
        <LoadMore onClick={() => void archive.fetchNextPage()} pending={archive.isFetchingNextPage} />
      ) : null}
    </Dialog>
  );
}
