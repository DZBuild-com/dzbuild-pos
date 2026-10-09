import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PinInput } from "@/components/pin-pad";
import { Splash } from "@/components/splash";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { formatDate } from "@/lib/format";
import { useLicenseStatus, useRemoveLicense } from "@/lib/ipc/license";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { ActivationForm } from "./ActivationForm";
import { DeviceCode } from "./DeviceCode";
import { SellerContact } from "./SellerContact";
import { stateNoticeKey } from "./license-text";

/** Settings › Licence: this PC's licence, its code, another key, and the owner's removal. Every
 * signed-in user sees it; Rust checks the owner and the PIN. */
export function LicenseSettings({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const status = useLicenseStatus();
  const [removing, setRemoving] = useState(false);
  if (!status.data) return <Splash inline />;
  const data = status.data;
  const licence = data.licence;
  const reason = stateNoticeKey(data);
  return (
    <div className="flex flex-col gap-6">
      <Card>
        <h2 className="text-2xl font-bold">{t("license:settings.title")}</h2>
        {reason ? (
          <p role="alert" className="rounded-control border border-warn bg-warn-soft p-3 text-ink">
            {t(reason, { code: data.previousDevice ?? "" })}
          </p>
        ) : null}
        <dl className="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2">
          <dt className="font-semibold">{t("license:settings.status")}</dt>
          <dd>{t(`license:settings.states.${data.state}`)}</dd>
          {licence ? (
            <>
              <dt className="font-semibold">{t("license:settings.kind")}</dt>
              <dd>{t(`license:settings.kinds.${licence.kind}`)}</dd>
              {licence.customer?.name ? (
                <>
                  <dt className="font-semibold">{t("license:settings.customer")}</dt>
                  <dd>{licence.customer.name}</dd>
                </>
              ) : null}
              <dt className="font-semibold">{t("license:settings.issued")}</dt>
              <dd>
                <bdi dir="ltr">{formatDate(licence.issuedAt, lang)}</bdi>
              </dd>
              <dt className="font-semibold">{t("license:settings.expires")}</dt>
              <dd>
                {licence.expiresAt === null ? (
                  t("license:settings.never")
                ) : (
                  <>
                    <bdi dir="ltr">{formatDate(licence.expiresAt, lang)}</bdi>
                    {data.daysLeft === null ? null : (
                      <span className="text-ink-2">
                        {" "}
                        · {t("license:settings.daysLeft", { count: data.daysLeft })}
                      </span>
                    )}
                  </>
                )}
              </dd>
            </>
          ) : null}
        </dl>
      </Card>
      <Card>
        <DeviceCode status={data} />
        <SellerContact seller={data.seller} />
      </Card>
      {data.state === "demo" ? null : (
        <Card>
          <h2 className="text-xl font-bold">
            {t(licence ? "license:settings.replace" : "license:activation.step2")}
          </h2>
          <ActivationForm />
        </Card>
      )}
      {session.isOwner && licence && data.state !== "demo" ? (
        <div>
          <Button variant="danger" onClick={() => setRemoving(true)}>
            {t("license:settings.remove")}
          </Button>
        </div>
      ) : null}
      {removing ? <RemoveDialog onClose={() => setRemoving(false)} /> : null}
    </div>
  );
}

/** The owner's PIN again; the PC then reads only (the key stays valid and can be activated again). */
function RemoveDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const remove = useRemoveLicense();
  const [pin, setPin] = useState("");
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t("license:settings.removeTitle")}
      description={t("license:settings.removeBody")}
      footer={
        <>
          <Button variant="outline" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="danger"
            disabled={pin.length < 4 || remove.isPending}
            onClick={() =>
              remove.mutate(pin, {
                onSuccess: () => {
                  toast.success(t("license:settings.removed"));
                  onClose();
                },
              })
            }
          >
            {t("license:settings.confirmRemove")}
          </Button>
        </>
      }
    >
      <Field id="license-remove-pin" label={t("license:settings.pin")}>
        <PinInput id="license-remove-pin" value={pin} onChange={setPin} />
      </Field>
      {remove.error ? (
        <p role="alert" className="text-danger">
          {errorText(remove.error)}
        </p>
      ) : null}
    </Dialog>
  );
}
