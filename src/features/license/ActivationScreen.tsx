import { Store } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { initialSetupLang, rememberSetupLang } from "@/features/setup/setup-lang";
import { markUrl } from "@/lib/brand";
import { LANGS, currentLang, setLanguage } from "@/lib/i18n";
import { useEnterDemo } from "@/lib/ipc/demo";
import type { LicenseStatus } from "@/lib/ipc/types";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { cn } from "@/lib/utils";
import { ActivationForm } from "./ActivationForm";
import { DeviceCode } from "./DeviceCode";
import { SellerContact } from "./SellerContact";
import { stateNoticeKey } from "./license-text";

/** A new install without a valid licence: this PC's code to send, the key to paste or the
 * `.dzlic` to load, the seller's contact, and the demo shop. The setup wizard comes after it; the
 * language follows the wizard's rules (this PC's last choice, else Windows'). */
export function ActivationScreen({ status }: { status: LicenseStatus }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const enterDemo = useEnterDemo();
  useEffect(() => {
    const wanted = initialSetupLang();
    if (wanted !== currentLang()) void setLanguage(wanted);
  }, []);
  const notice = stateNoticeKey(status);
  const openingDemo = enterDemo.isPending || enterDemo.isSuccess;
  return (
    <div className="min-h-dvh bg-bg px-4 pt-8 pb-[calc(2rem+var(--osk-inset,0px))] text-ink">
      <main className="mx-auto flex max-w-2xl flex-col gap-6">
        <header className="flex flex-wrap items-center gap-3">
          <img src={markUrl} alt="" className="size-12" />
          <h1 className="min-w-0 flex-1 text-3xl font-bold">{t("license:activation.title")}</h1>
          <div role="group" aria-label={t("license:activation.language")} className="flex gap-2">
            {LANGS.map((option) => (
              <Button
                key={option}
                variant="outline"
                lang={option}
                aria-pressed={lang === option}
                className={cn(lang === option && "border-brand bg-brand-soft")}
                onClick={() => {
                  rememberSetupLang(option);
                  void setLanguage(option);
                }}
              >
                {t(`common:languages.${option}`)}
              </Button>
            ))}
          </div>
        </header>
        <p className="text-ink-2">{t("license:activation.intro")}</p>
        {notice ? (
          <p role="alert" className="rounded-control border border-warn bg-warn-soft p-3 text-ink">
            {t(notice, { code: status.previousDevice ?? "" })}
          </p>
        ) : null}
        <Card>
          <h2 className="text-xl font-bold">{t("license:activation.step1")}</h2>
          <DeviceCode status={status} />
          <SellerContact seller={status.seller} />
        </Card>
        <Card>
          <h2 className="text-xl font-bold">{t("license:activation.step2")}</h2>
          <ActivationForm />
        </Card>
        <Card>
          <Button
            size="lg"
            variant="outline"
            className="justify-start"
            disabled={openingDemo}
            onClick={() => {
              rememberSetupLang(lang);
              enterDemo.mutate(lang);
            }}
          >
            <Store aria-hidden />
            {t("license:activation.demo")}
          </Button>
          <p className="text-sm text-ink-2">{t("license:activation.demoHint")}</p>
          {openingDemo ? (
            <p role="status" className="font-semibold text-info">
              {t("license:activation.openingDemo")}
            </p>
          ) : null}
          {enterDemo.error ? (
            <p role="alert" className="text-danger">
              {errorText(enterDemo.error)}
            </p>
          ) : null}
        </Card>
      </main>
    </div>
  );
}
