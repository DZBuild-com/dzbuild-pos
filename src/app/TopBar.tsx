import { LogOut } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Avatar } from "@/components/avatar";
import { LanguageToggle } from "@/components/language-toggle";
import { ThemeToggle } from "@/components/theme-toggle";
import { Button } from "@/components/ui/button";
import { useLogout, useSetMyLanguage } from "@/lib/ipc/auth";
import { useCompany } from "@/lib/ipc/company";
import { useSetTheme, useThemePreference } from "@/lib/ipc/settings";
import type { Session } from "@/lib/ipc/types";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";
import { StatusPills } from "./StatusPills";

export function TopBar({ session }: { session: Session }) {
  const { t } = useTranslation();
  const lang = useLang();
  const company = useCompany();
  const setLanguage = useSetMyLanguage();
  const logout = useLogout();
  const theme = useThemePreference();
  const setTheme = useSetTheme();
  const shopName = company.data ? localizedName(company.data, lang) : "";
  const roleName = localizedName(
    { nameAr: session.roleNameAr, nameFr: session.roleNameFr, nameEn: session.roleNameEn },
    lang,
  );
  return (
    <header className="flex min-h-16 items-center gap-3 border-b border-line bg-surface px-4">
      <p className="truncate text-lg font-bold">{shopName || t("common:appName")}</p>
      <span className="rounded-full bg-surface-2 px-3 py-1 text-sm font-semibold">
        {t("common:terminal.code", { code: session.terminalCode })}
      </span>
      <StatusPills session={session} />
      <div className="ms-auto flex items-center gap-2">
        <LanguageToggle value={lang} onChange={(next) => setLanguage.mutate(next)} />
        <ThemeToggle value={theme} onChange={setTheme} />
        <div className="flex items-center gap-2 ps-2">
          <Avatar name={session.displayName} />
          <div className="flex flex-col leading-tight max-[1100px]:hidden">
            <span className="font-semibold">{session.displayName}</span>
            <span className="text-sm text-ink-2">{roleName}</span>
          </div>
        </div>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("common:actions.logout")}
          title={t("common:actions.logout")}
          onClick={() => logout.mutate()}
        >
          <LogOut aria-hidden className="rtl:-scale-x-100" />
        </Button>
      </div>
    </header>
  );
}
