import { Archive, Pencil, Plus } from "lucide-react";
import { type FormEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { NumericInput } from "@/components/numeric-input";
import { QrCode } from "@/components/qr-code";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { HostOnlyNote } from "@/features/network/HostOnlyNote";
import {
  useArchivePaymentType,
  useBaridpayQr,
  usePaymentTypes,
  useSavePaymentType,
  useSetBaridpayQr,
} from "@/lib/ipc/completion";
import { useTillLink } from "@/lib/ipc/lan";
import { useSetting } from "@/lib/ipc/settings";
import type { PaymentMethod, PaymentType } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The tenders a custom type can ride (pos-core `payment_types::clean`). */
const BASE_METHODS: readonly PaymentMethod[] = ["card_cib", "edahabia", "baridimob", "cheque", "transfer"];
const CODE = /^[a-z0-9_]{1,24}$/;
const QR_MIN = 10;
const QR_MAX = 512;

/** Settings › Paiements (PAY-006, PAY-007, HW-042): the custom payment types and the BaridPay QR
 * the customer screen shows. The main PC's master data: read-only on a till. */
export function PaymentsSettings() {
  return (
    <div className="flex flex-col gap-6">
      <HostOnlyNote />
      <PaymentTypes />
      <BaridpayQr />
    </div>
  );
}

function PaymentTypes() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const till = useTillLink();
  const [showArchived, setShowArchived] = useState(false);
  const types = usePaymentTypes(showArchived);
  const archive = useArchivePaymentType();
  const [editing, setEditing] = useState<PaymentType | "new" | null>(null);
  const rows = types.data ?? [];
  return (
    <Card aria-labelledby="payment-types-title">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 id="payment-types-title" className="text-xl font-bold">
          {t("config:payments.types.title")}
        </h2>
        {till.isTill ? null : (
          <Button variant="primary" onClick={() => setEditing("new")}>
            <Plus aria-hidden />
            {t("config:payments.types.add")}
          </Button>
        )}
      </div>
      <p className="text-ink-2">{t("config:payments.types.intro")}</p>
      <label className="flex min-h-11 items-center gap-3">
        <Checkbox checked={showArchived} onCheckedChange={(value) => setShowArchived(value === true)} />
        <span>{t("config:payments.types.showArchived")}</span>
      </label>
      {types.error ? (
        <p role="alert" className="text-danger">
          {errorText(types.error)}
        </p>
      ) : rows.length === 0 ? (
        <p className="text-ink-2">
          {types.isPending ? t("common:status.loading") : t("config:payments.types.empty")}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse">
            <thead className="text-sm text-ink-2">
              <tr className="h-10">
                <th scope="col" className="text-start font-semibold">
                  {t("config:payments.types.code")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("config:payments.types.nameFr")}
                </th>
                <th scope="col" className="text-start font-semibold">
                  {t("config:payments.types.base")}
                </th>
                <th scope="col" className="text-end font-semibold">
                  {t("config:payments.types.order")}
                </th>
                <th scope="col">
                  <span className="sr-only">{t("settings:users.columns.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((type) => (
                <tr key={type.id} className="h-12 border-t border-line even:bg-surface-2">
                  <td>
                    <bdi dir="ltr">{type.code}</bdi>
                  </td>
                  <td>
                    <span className="flex flex-wrap items-center gap-2">
                      {localizedName(type, lang)}
                      {type.requiresReference ? (
                        <Badge tone="info">{t("config:payments.types.referenceBadge")}</Badge>
                      ) : null}
                      {type.archived ? <Badge>{t("config:payments.types.archivedBadge")}</Badge> : null}
                    </span>
                  </td>
                  <td>{t(`payments:tenders.${type.baseMethod}`)}</td>
                  <td className="text-end tabular-nums">{type.sortOrder}</td>
                  <td>
                    {till.isTill || type.archived ? null : (
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label={`${t("common:actions.edit")} ${type.code}`}
                          title={t("common:actions.edit")}
                          onClick={() => setEditing(type)}
                        >
                          <Pencil aria-hidden />
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          aria-label={`${t("config:payments.types.archive")} ${type.code}`}
                          title={t("config:payments.types.archive")}
                          disabled={archive.isPending}
                          onClick={() =>
                            archive.mutate(type.id, {
                              onSuccess: () => toast.success(t("config:payments.types.archived")),
                              onError: (error) => toast.error(errorText(error)),
                            })
                          }
                        >
                          <Archive aria-hidden />
                        </Button>
                      </div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {editing ? (
        <PaymentTypeDialog type={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      ) : null}
    </Card>
  );
}

/** A custom type: its code, three names, the tender it rides, whether the reference is required
 * and its place among the tiles. */
function PaymentTypeDialog({ type, onClose }: { type: PaymentType | null; onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const save = useSavePaymentType();
  const [code, setCode] = useState(type?.code ?? "");
  const [names, setNames] = useState({
    nameAr: type?.nameAr ?? "",
    nameFr: type?.nameFr ?? "",
    nameEn: type?.nameEn ?? "",
  });
  const [baseMethod, setBaseMethod] = useState<PaymentMethod>(type?.baseMethod ?? "transfer");
  const [requiresReference, setRequiresReference] = useState(type?.requiresReference ?? true);
  const [order, setOrder] = useState(String(type?.sortOrder ?? 0));
  const [tried, setTried] = useState(false);
  const codeOk = CODE.test(code.trim());
  const namesOk = [names.nameAr, names.nameFr, names.nameEn].some((n) => n.trim() !== "");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (!codeOk || !namesOk) return;
    save.mutate(
      {
        id: type?.id ?? null,
        input: {
          code: code.trim(),
          nameAr: names.nameAr.trim(),
          nameFr: names.nameFr.trim(),
          nameEn: names.nameEn.trim(),
          baseMethod,
          requiresReference,
          sortOrder: order === "" ? 0 : Number(order),
        },
      },
      {
        onSuccess: () => {
          toast.success(t("config:payments.types.saved"));
          onClose();
        },
      },
    );
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => (open ? undefined : onClose())}
      title={t(type ? "config:payments.types.edit" : "config:payments.types.add")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button type="submit" form="payment-type-form" variant="primary" disabled={save.isPending}>
            {t("config:payments.types.save")}
          </Button>
        </>
      }
    >
      <form id="payment-type-form" noValidate onSubmit={submit} className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id="payment-type-code"
            label={t("config:payments.types.code")}
            hint={t("config:payments.types.codeHint")}
            error={tried && !codeOk ? t("config:payments.types.codeInvalid") : undefined}
          >
            <Input
              id="payment-type-code"
              dir="ltr"
              maxLength={24}
              value={code}
              onChange={(event) => setCode(event.target.value.toLowerCase())}
            />
          </Field>
          <Field id="payment-type-base" label={t("config:payments.types.base")}>
            <NativeSelect
              id="payment-type-base"
              value={baseMethod}
              onChange={(event) => {
                const chosen = BASE_METHODS.find((m) => m === event.target.value);
                if (chosen) setBaseMethod(chosen);
              }}
            >
              {BASE_METHODS.map((m) => (
                <option key={m} value={m}>
                  {t(`payments:tenders.${m}`)}
                </option>
              ))}
            </NativeSelect>
          </Field>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field id="payment-type-ar" label={t("config:payments.types.nameAr")}>
            <Input
              id="payment-type-ar"
              dir="rtl"
              lang="ar"
              maxLength={80}
              value={names.nameAr}
              onChange={(event) => setNames({ ...names, nameAr: event.target.value })}
            />
          </Field>
          <Field
            id="payment-type-fr"
            label={t("config:payments.types.nameFr")}
            error={tried && !namesOk ? t("validation:nameRequired") : undefined}
          >
            <Input
              id="payment-type-fr"
              dir="ltr"
              maxLength={80}
              value={names.nameFr}
              onChange={(event) => setNames({ ...names, nameFr: event.target.value })}
            />
          </Field>
          <Field id="payment-type-en" label={t("config:payments.types.nameEn")}>
            <Input
              id="payment-type-en"
              dir="ltr"
              maxLength={80}
              value={names.nameEn}
              onChange={(event) => setNames({ ...names, nameEn: event.target.value })}
            />
          </Field>
        </div>
        <div className="grid items-end gap-4 sm:grid-cols-2">
          <label className="flex min-h-11 items-center gap-3 font-semibold">
            <Checkbox
              checked={requiresReference}
              onCheckedChange={(value) => setRequiresReference(value === true)}
            />
            <span>{t("config:payments.types.reference")}</span>
          </label>
          <Field id="payment-type-order" label={t("config:payments.types.order")}>
            <NumericInput
              id="payment-type-order"
              kind="integer"
              maxLength={3}
              className="max-w-32"
              value={order}
              onValueChange={setOrder}
            />
          </Field>
        </div>
        {save.error ? (
          <p role="alert" className="text-danger">
            {errorText(save.error)}
          </p>
        ) : null}
      </form>
    </Dialog>
  );
}

/** HW-042: the text of the shop's BaridPay merchant QR, and the QR as the customer screen shows
 * it while the payment dialog has BaridiMob selected (drawn from the backend's matrix). */
function BaridpayQr() {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const till = useTillLink();
  const stored = useSetting("payments.baridpay_qr");
  const qr = useBaridpayQr();
  const save = useSetBaridpayQr();
  const [typed, setTyped] = useState<string | null>(null);
  const [tried, setTried] = useState(false);
  const text = typed ?? (typeof stored.data === "string" ? stored.data : "");
  const length = [...text.trim()].length;
  const valid = length >= QR_MIN && length <= QR_MAX;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    setTried(true);
    if (!valid) return;
    save.mutate(text.trim(), {
      onSuccess: () => {
        setTyped(null);
        toast.success(t("config:payments.qr.saved"));
      },
    });
  };
  return (
    <Card aria-labelledby="baridpay-title">
      <h2 id="baridpay-title" className="text-xl font-bold">
        {t("config:payments.qr.title")}
      </h2>
      <p className="text-ink-2">{t("config:payments.qr.intro")}</p>
      <div className="flex flex-col gap-6 md:flex-row">
        <form noValidate onSubmit={submit} className="flex min-w-0 flex-1 flex-col gap-4">
          <Field
            id="baridpay-text"
            label={t("config:payments.qr.text")}
            hint={t("config:payments.qr.hint")}
            error={tried && !valid ? t("config:payments.qr.invalid") : undefined}
          >
            <textarea
              id="baridpay-text"
              dir="ltr"
              rows={4}
              maxLength={QR_MAX}
              disabled={till.isTill}
              value={text}
              onChange={(event) => {
                setTyped(event.target.value);
                setTried(false);
              }}
              className="w-full rounded-control border border-line bg-surface px-3 py-2 font-mono text-sm text-ink disabled:opacity-50"
            />
          </Field>
          {save.error ? (
            <p role="alert" className="text-danger">
              {errorText(save.error)}
            </p>
          ) : null}
          {till.isTill ? null : (
            <div className="flex flex-wrap gap-2">
              <Button type="submit" variant="primary" disabled={save.isPending}>
                {t("config:payments.qr.save")}
              </Button>
              {typeof stored.data === "string" ? (
                <Button
                  variant="outline"
                  disabled={save.isPending}
                  onClick={() =>
                    save.mutate(null, {
                      onSuccess: () => {
                        setTyped(null);
                        toast.success(t("config:payments.qr.cleared"));
                      },
                    })
                  }
                >
                  {t("config:payments.qr.clear")}
                </Button>
              ) : null}
            </div>
          )}
        </form>
        <div className="flex flex-col items-center gap-2">
          <p className="font-semibold">{t("config:payments.qr.preview")}</p>
          {qr.data ? (
            <>
              <QrCode matrix={qr.data} size={200} label={t("hardware:customerDisplay.payBaridimob")} />
              <p className="font-semibold">{t("hardware:customerDisplay.payBaridimob")}</p>
            </>
          ) : (
            <p className="max-w-56 text-center text-ink-2">{t("config:payments.qr.none")}</p>
          )}
        </div>
      </div>
    </Card>
  );
}
