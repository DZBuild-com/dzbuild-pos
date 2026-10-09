import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { formatNumber } from "@/lib/format";
import { useShowOnDisplay } from "@/lib/ipc/devices";
import { useBaridpayQr } from "@/lib/ipc/till-packs";
import type { PaymentType } from "@/lib/ipc/types";
import { usePointsPreview } from "@/lib/ipc/till-packs";
import type { DisplayFrame, PaymentInput, PaymentMethod } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";
import type { Tender } from "./payment-model";
import { useTillKit } from "./till-kit";
import { VoucherTender } from "./VoucherTender";

/** Tiles never take the focus from the amount being typed. */
const keepFocus = (event: { preventDefault: () => void }) => event.preventDefault();

/** HW-042: while BaridiMob is the tender being typed, the customer screen shows the shop's
 * BaridPay QR over the sale's frame; leaving BaridiMob (or the dialog) sends the frame without. */
function useBaridpayOnDisplay(active: boolean): void {
  const { lastDisplayFrame } = useTillKit();
  const show = useShowOnDisplay();
  const qr = useBaridpayQr(active).data ?? null;
  useEffect(() => {
    if (!active || !qr) return undefined;
    const frame = lastDisplayFrame();
    if (!frame) return undefined;
    // `payQr` is a field of `DisplayFrame` (Task 21); the merge brings its type
    show({ ...frame, payQr: qr } as DisplayFrame);
    return () => show(frame);
  }, [active, qr, lastDisplayFrame, show]);
}

/**
 * The payment dialog's pack part, start column: the custom payment types after the base tenders
 * (core Task 16: a tile adds a tender of its base method carrying the type, asking its reference
 * when it requires one), the voucher reader, and the BaridPay QR on the customer screen.
 */
export function PaymentTiles({
  types,
  voucherOpen,
  onVoucherClose,
  customerId,
  storeCredit,
  rest,
  method,
  onType,
  onTender,
}: {
  types: PaymentType[];
  voucherOpen: boolean;
  onVoucherClose: () => void;
  customerId: string | null;
  storeCredit: number;
  rest: number | null;
  method: PaymentMethod | undefined;
  onType: (type: PaymentType) => void;
  onTender: (tender: Tender) => void;
}) {
  const lang = useLang();
  useBaridpayOnDisplay(method === "baridimob");
  return (
    <>
      {types.length > 0 ? (
        <div className="grid grid-cols-4 gap-2">
          {types.map((type) => (
            <button
              key={type.id}
              type="button"
              onMouseDown={keepFocus}
              onClick={() => onType(type)}
              className="min-h-14 rounded-control border border-line bg-surface-2 px-2 font-semibold"
            >
              {localizedName(type, lang)}
            </button>
          ))}
        </div>
      ) : null}
      {voucherOpen ? (
        <VoucherTender
          customerId={customerId}
          storeCredit={storeCredit}
          rest={rest}
          onUse={(tender) => {
            onTender(tender);
            onVoucherClose();
          }}
          onClose={onVoucherClose}
        />
      ) : null}
    </>
  );
}

/** LOY-002: "Points gagnés : +23" under the totals, the backend's figure for an enrolled
 * customer (nothing for a customer without a live card). */
export function PointsLine({
  customerId,
  totalTtc,
  payments,
}: {
  customerId: string;
  totalTtc: number | undefined;
  payments: PaymentInput[];
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const points = usePointsPreview(customerId, totalTtc, payments).data;
  if (points === undefined || points === null) return null;
  return (
    <p className="rounded-control bg-ok-soft p-2 font-semibold text-ok">
      {t("packs:till.points", { points: formatNumber(points, lang) })}
    </p>
  );
}
