import { Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { QrCode } from "@/components/qr-code";
import { Button } from "@/components/ui/button";
import type { LicenseStatus } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";

/** This PC's code, large and left to right in every language, with a copy button and the QR the
 * customer scans to send it (a WhatsApp message to the seller when one is configured). */
export function DeviceCode({ status }: { status: LicenseStatus }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const code = status.deviceCode;
  if (!code) return null;
  const copy = () => {
    navigator.clipboard
      .writeText(code)
      .then(() => toast.success(t("license:device.copied")))
      .catch((error: unknown) => toast.error(errorText(error)));
  };
  return (
    <div className="flex flex-wrap items-center gap-6">
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <p className="font-semibold">{t("license:device.label")}</p>
        <bdi
          dir="ltr"
          data-testid="device-code"
          className="text-[2rem] leading-tight font-bold break-words tabular-nums select-all"
        >
          {code}
        </bdi>
        <div>
          <Button variant="outline" onClick={copy}>
            <Copy aria-hidden />
            {t("license:device.copy")}
          </Button>
        </div>
      </div>
      {status.qr ? (
        <div className="flex flex-col items-center gap-2">
          <QrCode matrix={status.qr} size={160} label={t("license:device.qr")} />
          <p className="max-w-48 text-center text-sm text-ink-2">
            {t(status.seller.whatsapp ? "license:activation.scan" : "license:activation.scanCode")}
          </p>
        </div>
      ) : null}
    </div>
  );
}
