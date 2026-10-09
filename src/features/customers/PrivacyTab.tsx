import { Download, UserX } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApproval } from "@/components/manager-approval";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { isApprovalCancelled } from "@/lib/approval";
import { formatDate } from "@/lib/format";
import { useSession } from "@/lib/ipc/auth";
import { toAppError } from "@/lib/ipc/client";
import {
  CONSENT_CHANNELS,
  CONSENT_SOURCES,
  type Consent,
  type ConsentChannel,
  type ConsentSource,
  useAnonymise,
  useConsents,
  useExportCustomerData,
  useSetConsent,
} from "@/lib/ipc/privacy";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** What `customer_has_open_items` names (`privacy::open_items`), in its words. */
const OPEN_ITEMS = [
  "balance",
  "deposits",
  "orders",
  "serviceTickets",
  "vouchers",
  "appointments",
  "instalmentGuarantees",
  "voucherBuyer",
] as const;

/** The customer page's "Données personnelles" tab (PDP-002, PDP-004): the consent per channel
 * with its history, the export of everything the shop keeps about the customer, and the
 * anonymisation that keeps the fiscal and legal records. */
export function PrivacyTab({ customerId }: { customerId: string }) {
  const { t } = useTranslation();
  const permissions = useSession().data?.permissions ?? [];
  const manages = permissions.includes("customers.manage");
  return (
    <div className="flex flex-col gap-6">
      <p className="text-ink-2">{t("privacy:intro")}</p>
      <Consents customerId={customerId} />
      <div className="flex flex-wrap gap-2">
        {manages ? <ExportButton customerId={customerId} /> : null}
        <AnonymiseButton customerId={customerId} />
      </div>
    </div>
  );
}

/** The latest answer of each channel: `consents` is newest first. */
function latest(consents: readonly Consent[], channel: ConsentChannel): Consent | undefined {
  return consents.find((c) => c.channel === channel);
}

function Consents({ customerId }: { customerId: string }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const consents = useConsents(customerId);
  const set = useSetConsent();
  const [source, setSource] = useState<ConsentSource>("counter");
  const rows = consents.data ?? [];
  const answer = (c: Consent | undefined) =>
    c === undefined
      ? t("privacy:consent.unknown")
      : t(c.granted ? "privacy:consent.granted" : "privacy:consent.refused");
  return (
    <Card aria-labelledby="privacy-consents-title">
      <h2 id="privacy-consents-title" className="text-xl font-bold">
        {t("privacy:consent.title")}
      </h2>
      <Field id="privacy-source" label={t("privacy:consent.source.label")}>
        <NativeSelect
          id="privacy-source"
          className="max-w-72"
          value={source}
          onChange={(event) => {
            const chosen = CONSENT_SOURCES.find((s) => s === event.target.value);
            if (chosen) setSource(chosen);
          }}
        >
          {CONSENT_SOURCES.map((s) => (
            <option key={s} value={s}>
              {t(`privacy:consent.source.${s}`)}
            </option>
          ))}
        </NativeSelect>
      </Field>
      <div className="flex flex-col divide-y divide-line">
        {CONSENT_CHANNELS.map((channel) => {
          const now = latest(rows, channel);
          return (
            <label key={channel} className="flex min-h-12 items-center gap-3">
              <input
                type="checkbox"
                role="switch"
                className="size-6 accent-brand"
                checked={now?.granted === true}
                disabled={consents.isPending || set.isPending}
                onChange={(event) =>
                  set.mutate(
                    { customerId, channel, granted: event.target.checked, source, note: null },
                    { onSuccess: () => toast.success(t("privacy:consent.saved")) },
                  )
                }
              />
              <span className="font-semibold">{t(`privacy:consent.${channel}`)}</span>
              <span className="ms-auto text-ink-2">{answer(now)}</span>
            </label>
          );
        })}
      </div>
      {(consents.error ?? set.error) ? (
        <p role="alert" className="text-danger">
          {errorText(consents.error ?? set.error)}
        </p>
      ) : null}
      <h3 className="font-bold">{t("privacy:consent.history")}</h3>
      {rows.length === 0 ? (
        <p className="text-ink-2">{t("privacy:consent.historyEmpty")}</p>
      ) : (
        <ul className="flex flex-col">
          {rows.map((c) => (
            <li
              key={`${c.channel}-${c.createdAt}`}
              className="flex flex-wrap gap-x-3 border-b border-line py-2 last:border-b-0"
            >
              <bdi dir="ltr" className="text-ink-2">
                {formatDate(c.createdAt, lang, { withTime: true })}
              </bdi>
              <span>
                {t("privacy:consent.row", {
                  channel: t(`privacy:consent.${c.channel}`),
                  answer: answer(c),
                  source: t(`privacy:consent.source.${c.source}`),
                  name: c.userName,
                })}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

/** PDP-004: the JSON and the Excel workbook land in a folder the user picks (a Rust dialog). */
function ExportButton({ customerId }: { customerId: string }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const exportData = useExportCustomerData();
  return (
    <div className="flex flex-col gap-2">
      <Button
        variant="outline"
        className="self-start"
        disabled={exportData.isPending}
        onClick={() => exportData.mutate(customerId)}
      >
        <Download aria-hidden />
        {t("privacy:export")}
      </Button>
      <p className="text-sm text-ink-2">{t("privacy:exportHint")}</p>
      {exportData.data ? (
        <p role="status" className="rounded-control bg-ok-soft p-3 text-ok">
          {t("privacy:exported", { json: exportData.data.jsonPath, xlsx: exportData.data.xlsxPath })}
        </p>
      ) : null}
      {exportData.error ? (
        <p role="alert" className="text-danger">
          {errorText(exportData.error)}
        </p>
      ) : null}
    </div>
  );
}

/** The refusal's open items in words ("Solde du compte, commande en cours"), or the backend's
 * own message for any other error. */
function useRefusalText() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  return (error: unknown) => {
    const e = toAppError(error);
    if (e.code !== "customer_has_open_items") return errorText(error);
    const open = (e.details as { open?: unknown } | null | undefined)?.open;
    const items = (Array.isArray(open) ? open : [])
      .map((item) => OPEN_ITEMS.find((known) => known === item))
      .filter((item) => item !== undefined)
      .map((item) => t(`privacy:open.${item}`));
    return t("privacy:anonymise.refused", { items: items.join(t("privacy:open.separator")) });
  };
}

/** "Anonymiser" with `customers.manage`, or with a manager's approval; the confirmation names
 * what stays. */
function AnonymiseButton({ customerId }: { customerId: string }) {
  const { t } = useTranslation();
  const refusalText = useRefusalText();
  const anonymise = useAnonymise();
  const approval = useApproval();
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState<unknown>(null);
  const confirm = async () => {
    setError(null);
    try {
      const report = await approval.run(() =>
        anonymise.mutateAsync({ customerId, note: note.trim() === "" ? null : note.trim() }),
      );
      toast.success(t("privacy:anonymise.done", { code: report.code }));
      setOpen(false);
    } catch (e) {
      if (!isApprovalCancelled(e)) setError(e);
    }
  };
  return (
    <>
      <Button variant="danger" className="self-start" onClick={() => setOpen(true)}>
        <UserX aria-hidden />
        {t("privacy:anonymise.open")}
      </Button>
      <Dialog
        open={open && !approval.pending}
        onOpenChange={(next) => {
          setOpen(next);
          if (!next) setError(null);
        }}
        title={t("privacy:anonymise.title")}
        footer={
          <>
            <Button variant="outline" onClick={() => setOpen(false)}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="danger" disabled={anonymise.isPending} onClick={() => void confirm()}>
              {t("privacy:anonymise.confirm")}
            </Button>
          </>
        }
      >
        <p>{t("privacy:anonymise.body")}</p>
        <p className="rounded-control bg-info-soft p-3 text-info">{t("privacy:anonymise.kept")}</p>
        <Field
          id="privacy-anonymise-note"
          label={t("privacy:anonymise.note")}
          optional={t("customers:form.optional")}
        >
          <Input
            id="privacy-anonymise-note"
            dir="auto"
            maxLength={200}
            value={note}
            onChange={(event) => setNote(event.target.value)}
          />
        </Field>
        {error ? (
          <p role="alert" className="text-danger">
            {refusalText(error)}
          </p>
        ) : null}
      </Dialog>
      {approval.dialog}
    </>
  );
}
