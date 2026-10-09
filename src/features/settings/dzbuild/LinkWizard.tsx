import { Copy, ExternalLink, Smartphone } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { QrCode } from "@/components/qr-code";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  useCancelLink,
  useChooseStore,
  useDeviceFlowStart,
  useLinkAttempt,
  useLinkStart,
} from "@/lib/ipc/cloud";
import type { Session } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/** The sign-in happens on dzbuild.com, in the system browser or on a phone: the till never sees
 * the password (spec §4.7). `relink`: the link waits for a new sign-in to the same store. */
export function LinkWizard({ session, relink = false }: { session: Session; relink?: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const start = useLinkStart();
  const phone = useDeviceFlowStart();
  const cancel = useCancelLink();
  const choose = useChooseStore();
  const [storeId, setStoreId] = useState<string | null>(null);
  const started = start.isSuccess || phone.isSuccess;
  const attempt = useLinkAttempt(started);
  // the start put its attempt in the cache; the poll keeps it current (null once it is gone)
  const view = started ? (attempt.data ?? null) : null;
  const state = view?.state;
  const qr = phone.data?.qr ?? null;
  const owner = session.isOwner;
  const ownerTitle = owner ? undefined : t("sync:link.ownerOnly");
  const reset = () => {
    cancel.mutate();
    start.reset();
    phone.reset();
    choose.reset();
    setStoreId(null);
  };
  const copy = (url: string) => {
    Promise.resolve()
      .then(() => navigator.clipboard.writeText(url))
      .then(() => toast.success(t("sync:link.copied")))
      .catch(() => toast.error(t("sync:link.copyFailed")));
  };
  return (
    <Card>
      <h3 className="text-lg font-semibold">{t(relink ? "sync:link.relinkTitle" : "sync:link.title")}</h3>
      {relink ? <p className="font-medium">{t("sync:link.relinkHelp")}</p> : null}
      <ul className="list-disc ps-6 text-ink-2">
        <li>{t("sync:link.what.catalogue")}</li>
        <li>{t("sync:link.what.stock")}</li>
        <li>{t("sync:link.what.sales")}</li>
        <li>{t("sync:link.what.orders")}</li>
      </ul>
      <p className="text-sm text-ink-2">{t("sync:link.never")}</p>
      {!view || state?.state === "cancelled" || state?.state === "failed" ? (
        <div className="flex flex-wrap gap-2">
          <Button
            variant="primary"
            disabled={!owner || start.isPending}
            title={ownerTitle}
            onClick={() => {
              phone.reset();
              start.mutate(lang);
            }}
          >
            <ExternalLink aria-hidden className="rtl:-scale-x-100" />
            {t("sync:link.browser")}
          </Button>
          <Button
            variant="outline"
            disabled={!owner || phone.isPending}
            title={ownerTitle}
            onClick={() => {
              start.reset();
              phone.mutate();
            }}
          >
            <Smartphone aria-hidden />
            {t("sync:link.phone")}
          </Button>
        </div>
      ) : null}
      {state?.state === "failed" ? (
        <p role="alert" className="text-danger">
          {errorText({ code: state.code })}
        </p>
      ) : null}
      {start.error || phone.error ? (
        <p role="alert" className="text-danger">
          {errorText(start.error ?? phone.error)}
        </p>
      ) : null}
      {state?.state === "waiting" && view?.kind === "browser" ? (
        <div className="flex flex-col gap-2" role="status">
          <p>{t("sync:link.waitingBrowser")}</p>
          <div className="flex flex-wrap gap-2">
            {view.authorizeUrl ? (
              <Button variant="outline" onClick={() => view.authorizeUrl && copy(view.authorizeUrl)}>
                <Copy aria-hidden />
                {t("sync:link.copy")}
              </Button>
            ) : null}
            <Button variant="ghost" onClick={reset}>
              {t("common:actions.cancel")}
            </Button>
          </div>
        </div>
      ) : null}
      {state?.state === "waiting" && view?.kind === "device" && view.device ? (
        <div className="flex flex-wrap items-center gap-6" role="status">
          {qr ? <QrCode matrix={qr} label={t("sync:link.qrLabel")} /> : null}
          <div className="flex flex-col gap-2">
            <p>{t("sync:link.waitingPhone", { url: view.device.verificationUri })}</p>
            <p className="font-mono text-3xl font-bold tabular-nums">
              <bdi dir="ltr">{view.device.userCode}</bdi>
            </p>
            <div>
              <Button variant="ghost" onClick={reset}>
                {t("common:actions.cancel")}
              </Button>
            </div>
          </div>
        </div>
      ) : null}
      {state?.state === "finishing" || state?.state === "linked" || choose.isPending ? (
        <p role="status">{t("sync:link.finishing")}</p>
      ) : null}
      {state?.state === "chooseStore" && !choose.isPending ? (
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-2 font-semibold">{t("sync:link.chooseStore")}</legend>
          {state.stores.map((s) => (
            <label
              key={s.storeId}
              className="flex min-h-11 items-center gap-3 rounded-control border border-line px-3"
            >
              <input
                type="radio"
                name="dzbuild-store"
                value={s.storeId}
                checked={storeId === s.storeId}
                onChange={() => setStoreId(s.storeId)}
                className="size-5 accent-brand"
              />
              <span className="grow">{s.storeName}</span>
              {s.plan ? (
                <span className="text-sm text-ink-2">{t("sync:link.plan", { plan: s.plan })}</span>
              ) : null}
            </label>
          ))}
          <div className="flex flex-wrap gap-2">
            <Button
              variant="primary"
              disabled={!storeId || choose.isPending}
              onClick={() => storeId && choose.mutate(storeId)}
            >
              {t("sync:link.linkThis")}
            </Button>
            <Button variant="ghost" onClick={reset}>
              {t("common:actions.cancel")}
            </Button>
          </div>
        </fieldset>
      ) : null}
      {choose.error ? (
        <p role="alert" className="text-danger">
          {errorText(choose.error)}
        </p>
      ) : null}
    </Card>
  );
}
