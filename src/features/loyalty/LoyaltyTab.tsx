import { CreditCard } from "lucide-react";
import { type FormEvent, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { type Columns, DataTable, columnHelper } from "@/components/data-table/data-table";
import { NumericInput } from "@/components/numeric-input";
import { RadioCards } from "@/components/radio-cards";
import { StatTile } from "@/components/stat-tile";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { dayToMs } from "@/lib/dates";
import { formatDate, formatDZD } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import {
  useAdjustPoints,
  useBlockCard,
  useEnrol,
  useLoyaltyAccount,
  useLoyaltyEntries,
} from "@/lib/ipc/loyalty";
import type { Account, LoyaltyCard, Permission, PointsEntry } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { RedeemDialog } from "./RedeemDialog";

/** An entry of the history with its place in it (the ledger has no id the UI receives). */
type EntryRow = PointsEntry & { n: number };

const col = columnHelper<EntryRow>();

/**
 * The customer's "Fidélité" tab (LOY-002 lite): the card (enrolled with a scanned, typed or
 * generated code; blocked when lost), the points and those expiring within 30 days, the points
 * history, "Échanger des points" into a loyalty voucher and "Ajuster" for `customers.manage`.
 * Rewards are loyalty vouchers only: there is no draw of any kind (PRM-025).
 */
export function LoyaltyTab({ customerId }: { customerId: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const account = useLoyaltyAccount(customerId);
  // the actions shown depend on the user: the tab waits for both
  const session = useSession();
  if (account.error) {
    return (
      <p role="alert" className="text-danger">
        {errorText(account.error)}
      </p>
    );
  }
  if (!account.data || !session.data) return <p className="text-ink-2">{t("common:status.loading")}</p>;
  return <LoyaltyAccount account={account.data} permissions={session.data.permissions} />;
}

type Open = "redeem" | "adjust" | "block" | null;

function LoyaltyAccount({ account, permissions }: { account: Account; permissions: Permission[] }) {
  const { t } = useTranslation();
  const lang = useLang();
  const can = (p: Permission) => permissions.includes(p);
  const entries = useLoyaltyEntries(can("customers.view") ? account.customerId : null);
  const [open, setOpen] = useState<Open>(null);
  const card = account.card;
  const rows = useMemo<EntryRow[]>(() => (entries.data ?? []).map((e, n) => ({ ...e, n })), [entries.data]);
  const columns = useMemo<Columns<EntryRow>>(
    () =>
      col.columns([
        col.accessor("businessDate", {
          header: () => t("loyalty:entries.date"),
          cell: (c) => <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
        col.accessor("kind", {
          header: () => t("loyalty:entries.kind"),
          cell: (c) => t(`loyalty:entries.${c.getValue()}`),
        }),
        col.accessor("points", {
          header: () => t("loyalty:entries.points"),
          meta: { align: "end" },
          cell: (c) => (
            <bdi dir="ltr" className={cn("font-semibold", c.getValue() < 0 && "text-danger")}>
              {c.getValue() > 0 ? `+${c.getValue()}` : c.getValue()}
            </bdi>
          ),
        }),
        col.accessor((e) => e.documentNumber ?? e.voucherCode ?? e.note ?? "", {
          id: "ref",
          header: () => t("loyalty:entries.ref"),
          enableSorting: false,
          cell: ({ row }) =>
            row.original.documentNumber || row.original.voucherCode ? (
              <bdi dir="ltr">{row.original.documentNumber ?? row.original.voucherCode}</bdi>
            ) : (
              <span dir="auto">{row.original.note ?? ""}</span>
            ),
        }),
        col.accessor((e) => e.expiresOn ?? "", {
          id: "expires",
          header: () => t("loyalty:entries.expires"),
          cell: (c) =>
            c.getValue() === "" ? null : <bdi dir="ltr">{formatDate(dayToMs(c.getValue()), lang)}</bdi>,
        }),
      ]),
    [t, lang],
  );

  return (
    <div className="flex flex-col gap-6">
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        <StatTile
          label={t("loyalty:account.balance")}
          value={t("loyalty:account.points", { count: account.balance })}
          hint={t("loyalty:account.pointValue", { amount: formatDZD(account.pointValue, lang) })}
        />
        {account.expiringSoon > 0 && account.nextExpiry ? (
          <p role="status" className="self-center rounded-control bg-warn-soft p-3 font-semibold text-warn">
            {t("loyalty:account.expiring", {
              count: account.expiringSoon,
              date: formatDate(dayToMs(account.nextExpiry), lang),
            })}
          </p>
        ) : null}
      </div>
      <CardPanel
        customerId={account.customerId}
        card={card}
        permissions={permissions}
        onBlock={() => setOpen("block")}
      />
      {card ? (
        <div className="flex flex-wrap items-center gap-2">
          {can("sales.create") ? (
            <Button variant="primary" disabled={!account.redeemable} onClick={() => setOpen("redeem")}>
              {t("loyalty:account.redeem")}
            </Button>
          ) : null}
          {can("customers.manage") ? (
            <Button variant="outline" onClick={() => setOpen("adjust")}>
              {t("loyalty:account.adjust")}
            </Button>
          ) : null}
          {account.redeemable ? null : <p className="text-ink-2">{t("loyalty:account.notRedeemable")}</p>}
        </div>
      ) : null}
      {can("customers.view") ? (
        <section className="flex flex-col gap-3">
          <h2 className="text-xl font-bold">{t("loyalty:entries.title")}</h2>
          <DataTable
            ariaLabel={t("loyalty:entries.title")}
            data={rows}
            columns={columns}
            getRowId={(e) => String(e.n)}
            empty={entries.isPending ? t("common:status.loading") : t("loyalty:entries.none")}
          />
        </section>
      ) : null}
      {open === "redeem" ? <RedeemDialog account={account} onClose={() => setOpen(null)} /> : null}
      {open === "adjust" ? (
        <AdjustDialog customerId={account.customerId} onClose={() => setOpen(null)} />
      ) : null}
      {open === "block" && card ? <BlockDialog card={card} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}

/** The live card with "Bloquer la carte", or the enrolment: a scanned or typed code, or one the
 * backend generates (an in-store EAN-13 `297…`). */
function CardPanel({
  customerId,
  card,
  permissions,
  onBlock,
}: {
  customerId: string;
  card: LoyaltyCard | null;
  permissions: Permission[];
  onBlock: () => void;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const enrol = useEnrol();
  const [code, setCode] = useState("");
  const submit = (typed: string | null) =>
    enrol.mutate(
      { customerId, code: typed },
      { onSuccess: (saved) => toast.success(t("loyalty:card.enrolled", { code: saved.code })) },
    );
  return (
    <Card>
      <h2 className="flex items-center gap-2 text-xl font-bold">
        <CreditCard aria-hidden className="size-6" />
        {t("loyalty:card.title")}
      </h2>
      {card ? (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <bdi dir="ltr" className="text-xl font-bold tabular-nums">
            {card.code}
          </bdi>
          {permissions.includes("customers.manage") ? (
            <Button variant="danger" onClick={onBlock}>
              {t("loyalty:card.block")}
            </Button>
          ) : null}
        </div>
      ) : (
        <>
          <p className="text-ink-2">{t("loyalty:card.none")}</p>
          {permissions.includes("sales.create") ? (
            <form
              noValidate
              className="flex flex-wrap items-end gap-3"
              onSubmit={(event: FormEvent) => {
                event.preventDefault();
                if (code.trim() !== "") submit(code.trim());
              }}
            >
              <div className="min-w-60 flex-1">
                <Field
                  id="loyalty-card-code"
                  label={t("loyalty:card.code")}
                  hint={t("loyalty:card.codeHint")}
                >
                  <Input
                    id="loyalty-card-code"
                    dir="ltr"
                    autoComplete="off"
                    maxLength={20}
                    value={code}
                    onChange={(event) => setCode(event.target.value)}
                  />
                </Field>
              </div>
              <Button type="submit" variant="primary" disabled={code.trim() === "" || enrol.isPending}>
                {t("loyalty:card.enrol")}
              </Button>
              <Button variant="outline" disabled={enrol.isPending} onClick={() => submit(null)}>
                {t("loyalty:card.generate")}
              </Button>
            </form>
          ) : null}
          {enrol.error ? (
            <p role="alert" className="text-danger">
              {errorText(enrol.error)}
            </p>
          ) : null}
        </>
      )}
    </Card>
  );
}

/** A lost or stolen card: the customer keeps his points and can be given a new one. */
function BlockDialog({ card, onClose }: { card: LoyaltyCard; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const block = useBlockCard();
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("loyalty:card.block")}
      description={t("loyalty:card.blockHint")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={block.isPending}
            onClick={() =>
              block.mutate(card.code, {
                onSuccess: () => {
                  toast.success(t("loyalty:card.blocked", { code: card.code }));
                  onClose();
                },
              })
            }
          >
            {t("loyalty:card.block")}
          </Button>
        </>
      }
    >
      <bdi dir="ltr" className="text-xl font-bold">
        {card.code}
      </bdi>
      {block.error ? (
        <p role="alert" className="text-danger">
          {errorText(block.error)}
        </p>
      ) : null}
    </Dialog>
  );
}

type Direction = "add" | "remove";

/** Points added or taken off by hand (a paper card carried over, a correction), with the reason
 * the journal keeps; never below zero (the backend refuses). */
function AdjustDialog({ customerId, onClose }: { customerId: string; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const adjust = useAdjustPoints();
  const [direction, setDirection] = useState<Direction>("add");
  const [points, setPoints] = useState("");
  const [note, setNote] = useState("");
  const [tried, setTried] = useState(false);
  const count = /^\d+$/.test(points) ? Number(points) : 0;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (count <= 0 || note.trim() === "") return;
    adjust.mutate(
      { customerId, points: direction === "add" ? count : -count, note: note.trim() },
      {
        onSuccess: () => {
          toast.success(t("loyalty:adjust.done"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t("loyalty:adjust.title")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="loyalty-adjust" variant="primary" disabled={adjust.isPending}>
            {t("loyalty:adjust.save")}
          </Button>
        </>
      }
    >
      <form id="loyalty-adjust" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <RadioCards
          legend={t("loyalty:adjust.direction")}
          name="loyalty-adjust-direction"
          value={direction}
          onChange={setDirection}
          options={[
            { value: "add", label: t("loyalty:adjust.add") },
            { value: "remove", label: t("loyalty:adjust.remove") },
          ]}
        />
        <Field
          id="loyalty-adjust-points"
          label={t("loyalty:adjust.points")}
          error={tried && count <= 0 ? t("validation:required") : undefined}
        >
          <NumericInput id="loyalty-adjust-points" kind="integer" value={points} onValueChange={setPoints} />
        </Field>
        <Field
          id="loyalty-adjust-note"
          label={t("loyalty:account.note")}
          error={tried && note.trim() === "" ? t("validation:required") : undefined}
        >
          <Input
            id="loyalty-adjust-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(event) => setNote(event.target.value)}
          />
        </Field>
        {adjust.error ? (
          <p role="alert" className="text-danger">
            {errorText(adjust.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}
