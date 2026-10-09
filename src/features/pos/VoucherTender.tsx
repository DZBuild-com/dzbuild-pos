import type { TFunction } from "i18next";
import { type KeyboardEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { ScanInput } from "@/components/scan-input";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { formatAmountInput, formatDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import { describeVoucher } from "@/lib/ipc/till-packs";
import type { VoucherInfo } from "@/lib/ipc/types";
import { normalizeDigits } from "@/lib/numeric";
import { OWN_SCAN } from "@/lib/scanner";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import type { Tender } from "./payment-model";

/** What a voucher reference spends, in a sentence ("Bon cadeau BA2026-000001 — solde …"). */
export function voucherText(info: VoucherInfo, t: TFunction, lang: Lang): string {
  const balance = formatDZD(info.balance, lang);
  switch (info.target.kind) {
    case "store_credit":
      return t("packs:till.voucher.storeCredit", { balance });
    case "deposit":
      return t("packs:till.voucher.deposit", { number: info.target.number, balance });
    case "gift_voucher":
      return t("packs:till.voucher.giftVoucher", { code: info.target.code, balance });
  }
}

/** The tender's reference: the voucher's or the deposit's number; none for store credit. */
function referenceOf(info: VoucherInfo): string {
  switch (info.target.kind) {
    case "store_credit":
      return "";
    case "deposit":
      return info.target.number;
    case "gift_voucher":
      return info.target.code;
  }
}

function voucherTender(amount: number, reference: string, lang: Lang): Tender {
  return {
    method: "voucher",
    amount: formatAmountInput(amount, lang),
    reference,
    chequeBank: "",
    chequeDate: "",
  };
}

/**
 * The voucher tile (SAL-061, LOY-001, ORD): a code typed or scanned is read by the backend
 * (`vouchers_describe`): a gift, exchange or loyalty voucher `BA…`, the deposit of an order `CC…`
 * or a repair `RP…`, with its balance; the customer's store credit is one tap. "Utiliser le bon"
 * adds a voucher tender of what it can pay, its balance or what is left to pay (the backend's
 * figures, checked again at "Valider").
 */
export function VoucherTender({
  customerId,
  storeCredit,
  rest,
  onUse,
  onClose,
}: {
  customerId: string | null;
  storeCredit: number;
  /** What is left to pay, the backend's; `null` while it prices the tenders. */
  rest: number | null;
  onUse: (tender: Tender) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const [code, setCode] = useState("");
  const [found, setFound] = useState<VoucherInfo | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const read = async (value: string) => {
    const reference = normalizeDigits(value.trim()).toUpperCase();
    if (reference === "") return;
    setBusy(true);
    setFound(null);
    setProblem(null);
    try {
      const info = await describeVoucher(reference);
      if (!info) setProblem(t("packs:till.voucher.unknown"));
      else if (info.balance <= 0) setProblem(t("packs:till.voucher.spent"));
      else if (info.customerId !== null && info.customerId !== customerId) {
        setProblem(t("packs:till.voucher.otherCustomer"));
      } else setFound(info);
    } catch (e) {
      setProblem(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const use = (balance: number, reference: string) => {
    if (rest === null || rest <= 0) return;
    onUse(voucherTender(Math.min(balance, rest), reference, lang));
  };

  return (
    <section
      aria-label={t("packs:till.voucher.title")}
      className="flex flex-col gap-3 rounded-card border border-brand p-3"
    >
      {/* inside the payment dialog's form: Enter reads the voucher, never validates the payment */}
      <div className="flex items-end gap-2">
        <div className="flex-1">
          <Field id="voucher-code" label={t("packs:till.voucher.code")} error={problem ?? undefined}>
            <ScanInput
              id="voucher-code"
              {...OWN_SCAN}
              dir="ltr"
              autoFocus
              maxLength={40}
              value={code}
              onChange={(event) => setCode(event.target.value)}
              onKeyDown={(event: KeyboardEvent) => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                event.stopPropagation();
                void read(code);
              }}
              onScan={(scanned) => {
                setCode(scanned);
                void read(scanned);
              }}
            />
          </Field>
        </div>
        <Button
          variant="secondary"
          disabled={busy}
          className={problem ? "mb-7" : undefined}
          onClick={() => void read(code)}
        >
          {t("packs:till.voucher.find")}
        </Button>
      </div>
      {found ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="font-semibold">{voucherText(found, t, lang)}</p>
          <Button
            variant="outline"
            disabled={rest === null || rest <= 0}
            onClick={() => use(found.balance, referenceOf(found))}
          >
            {t("packs:till.voucher.use")}
          </Button>
        </div>
      ) : null}
      {customerId !== null && storeCredit > 0 ? (
        <Button
          variant="outline"
          className="justify-start"
          disabled={rest === null || rest <= 0}
          onClick={() => use(storeCredit, "")}
        >
          {t("packs:till.voucher.storeCredit", { balance: formatDZD(storeCredit, lang) })}
        </Button>
      ) : null}
      <div>
        <Button variant="ghost" onClick={onClose}>
          {t("common:actions.cancel")}
        </Button>
      </div>
    </section>
  );
}
