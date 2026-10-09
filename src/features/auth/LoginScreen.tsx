import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Avatar } from "@/components/avatar";
import { LanguageToggle } from "@/components/language-toggle";
import { PinPad } from "@/components/pin-pad";
import { Button } from "@/components/ui/button";
import { DemoRibbon } from "@/app/DemoRibbon";
import { LicenseBanner } from "@/features/license/LicenseBanner";
import { markUrl } from "@/lib/brand";
import { useLogin, useUserTiles } from "@/lib/ipc/auth";
import { useCompany } from "@/lib/ipc/company";
import { useDemoStatus } from "@/lib/ipc/demo";
import { useLoginLanguage, useSetLoginLanguage } from "@/lib/ipc/settings";
import type { UserTile } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useApplyLanguage, useLang } from "@/lib/use-language";

/** User tiles, then a 64 px PIN pad; language toggles at the bottom (spec §10.5). */
export function LoginScreen() {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const tiles = useUserTiles();
  const company = useCompany();
  const login = useLogin();
  const loginLang = useLoginLanguage();
  const setLoginLang = useSetLoginLanguage();
  const [selected, setSelected] = useState<UserTile | null>(null);
  const [pin, setPin] = useState("");
  useApplyLanguage(loginLang);

  const shopName = company.data ? localizedName(company.data, lang) : "";
  const cancel = () => {
    setSelected(null);
    setPin("");
    login.reset();
  };
  const submit = () => {
    if (!selected) return;
    login.mutate({ user: selected.id, pin }, { onError: () => setPin("") });
  };

  return (
    <div className="flex min-h-dvh flex-col bg-bg text-ink">
      <DemoRibbon />
      <LicenseBanner withLink={false} />
      <header className="flex items-center gap-3 px-6 py-4">
        <img src={markUrl} alt="" className="size-10" />
        <p className="text-xl font-bold">{shopName || t("common:appName")}</p>
      </header>
      <main className="flex flex-1 flex-col items-center justify-center gap-6 px-4 pb-[var(--osk-inset,0px)]">
        {selected ? (
          <section className="flex w-full max-w-xs flex-col items-center gap-4">
            <Avatar name={selected.displayName} size="lg" />
            <h1 className="text-center text-2xl font-bold">
              {t("auth:login.pinTitle", { name: selected.displayName })}
            </h1>
            <PinPad
              value={pin}
              onChange={setPin}
              onSubmit={submit}
              onCancel={cancel}
              disabled={login.isPending}
              submitLabel={t("auth:login.submit")}
            />
            {login.error ? (
              <p role="alert" className="text-center text-danger">
                {errorText(login.error)}
              </p>
            ) : null}
            <Button
              variant="ghost"
              onClick={cancel}
              shortcut="Escape"
              shortcutLabel={t("common:keys.escape")}
            >
              {t("auth:login.changeUser")}
            </Button>
          </section>
        ) : (
          <section className="flex w-full max-w-4xl flex-col items-center gap-6">
            <h1 className="text-3xl font-bold">{t("auth:login.title")}</h1>
            <p className="text-ink-2">{t("auth:login.subtitle")}</p>
            {tiles.data && tiles.data.length === 0 ? <p role="alert">{t("auth:login.noUsers")}</p> : null}
            <ul className="grid w-full grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-4">
              {tiles.data?.map((tile) => (
                <li key={tile.id}>
                  <button
                    type="button"
                    onClick={() => setSelected(tile)}
                    className="flex w-full flex-col items-center gap-2 rounded-card border border-line bg-surface p-4 hover:bg-surface-2"
                  >
                    <Avatar name={tile.displayName} size="lg" />
                    <span className="text-lg font-semibold">{tile.displayName}</span>
                    <span className="text-sm text-ink-2">
                      {localizedName(
                        { nameAr: tile.roleNameAr, nameFr: tile.roleNameFr, nameEn: tile.roleNameEn },
                        lang,
                      )}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
            <DemoPins />
          </section>
        )}
      </main>
      <footer className="flex justify-center p-4">
        <LanguageToggle value={lang} onChange={setLoginLang} size="lg" />
      </footer>
    </div>
  );
}

/** Stand for the PINs while the sentence is split around them; never rendered. */
const OWNER_SLOT = "\uE000";
const CASHIER_SLOT = "\uE001";

/** ONB-002: the demo's PINs under the tiles, each in its own left-to-right island. */
function DemoPins() {
  const { t } = useTranslation();
  const demo = useDemoStatus().data;
  if (!demo?.active || demo.ownerPin === null || demo.cashierPin === null) return null;
  const pinOf = { [OWNER_SLOT]: demo.ownerPin, [CASHIER_SLOT]: demo.cashierPin };
  const parts = t("common:demo.pins", { owner: OWNER_SLOT, cashier: CASHIER_SLOT }).split(/(\uE000|\uE001)/u);
  return (
    <p className="rounded-control bg-warn-soft px-4 py-2 text-lg">
      {parts.map((part) =>
        part === OWNER_SLOT || part === CASHIER_SLOT ? (
          <bdi key={part} dir="ltr" className="font-bold tabular-nums">
            {pinOf[part]}
          </bdi>
        ) : (
          part
        ),
      )}
    </p>
  );
}
