import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { QrCode } from "@/components/qr-code";
import { formatDZD } from "@/lib/format";
import { type Lang, setLanguage } from "@/lib/i18n";
import { useCustomerFrame } from "@/lib/ipc/customer-display";
import type { DisplayFrame, DisplayShop } from "@/lib/ipc/types";
import { formatQty } from "@/lib/numeric";
import { useLanguagesLoaded } from "@/lib/use-language";

/** "Merci / شكراً / Thank you": the thanks in the three languages at once, whatever the ticket's. */
const THANKS_LANGS: readonly Lang[] = ["fr", "ar", "en"];

/** The shop's name in `lang`, else in the first language it has one in. */
function shopName(shop: DisplayShop | null, lang: Lang): string {
  if (!shop) return "";
  const own = lang === "ar" ? shop.nameAr : lang === "en" ? shop.nameEn : shop.nameFr;
  return own || shop.nameFr || shop.nameAr || shop.nameEn;
}

/** Spec §10.5 "Customer display" (HW-042): the shop, the current line, the amount due in 4 rem,
 * the thanks when paid. Every amount comes from the till's frame; nothing is computed here. */
export function CustomerDisplay() {
  const { t } = useTranslation();
  const { frame, shop } = useCustomerFrame();
  // The paid frame whose thanks are over: the next frame (another sale) is thanked again.
  const [thanked, setThanked] = useState<DisplayFrame | null>(null);
  const lang: Lang = frame?.lang ?? "fr";
  const thanksSeconds = shop?.thanksSeconds ?? 5;

  useEffect(() => {
    // UX-041: the screen speaks the ticket's language; also sets <html lang dir> (i18n.ts).
    void setLanguage(lang);
  }, [lang]);

  useEffect(() => {
    if (frame?.mode !== "paid") return undefined;
    const timer = window.setTimeout(() => setThanked(frame), thanksSeconds * 1_000);
    return () => window.clearTimeout(timer);
  }, [frame, thanksSeconds]);

  const idle = !frame || frame.mode === "idle" || (frame.mode === "paid" && thanked === frame);

  return (
    <main className="flex h-screen flex-col gap-6 bg-bg p-8 text-ink">
      <header className="flex items-center gap-6">
        {shop?.showLogo && shop.logo ? <img src={shop.logo} alt="" className="h-24 w-auto" /> : null}
        <h1 className="text-3xl font-bold">{shopName(shop, lang)}</h1>
      </header>
      {idle ? (
        <section className="flex flex-1 flex-col items-center justify-center gap-8">
          <p className="text-5xl font-bold">{t("hardware:customerDisplay.welcome")}</p>
          {shop?.storeQr && shop.storeUrl ? (
            <div className="flex flex-col items-center gap-2">
              <QrCode matrix={shop.storeQr} size={220} label={t("hardware:customerDisplay.orderOnline")} />
              <p className="text-xl">{t("hardware:customerDisplay.orderOnline")}</p>
              <bdi dir="ltr" className="text-lg">
                {shop.storeUrl.replace(/^https:\/\//, "").replace(/\/$/, "")}
              </bdi>
            </div>
          ) : null}
        </section>
      ) : frame.mode === "paid" ? (
        <Paid frame={frame} lang={lang} />
      ) : (
        <Sale frame={frame} lang={lang} />
      )}
    </main>
  );
}

/** Total, paid and change, then the thanks in the three languages. */
function Paid({ frame, lang }: { frame: DisplayFrame; lang: Lang }) {
  const { t } = useTranslation();
  // Arabic and English load on first use (src/lib/i18n.ts): the thanks wait for all three.
  const thanks = useLanguagesLoaded(THANKS_LANGS);
  return (
    <section className="flex flex-1 flex-col items-center justify-center gap-10">
      <dl className="grid grid-cols-2 items-baseline gap-x-8 gap-y-2 text-2xl tabular-nums">
        <dt>{t("hardware:customerDisplay.total")}</dt>
        <dd className="text-end font-bold">
          <bdi dir="ltr">{formatDZD(frame.total, lang)}</bdi>
        </dd>
        {frame.paid !== null ? (
          <>
            <dt>{t("hardware:customerDisplay.paid")}</dt>
            <dd className="text-end">
              <bdi dir="ltr">{formatDZD(frame.paid, lang)}</bdi>
            </dd>
          </>
        ) : null}
        {frame.change !== null && frame.change > 0 ? (
          <>
            <dt>{t("hardware:customerDisplay.change")}</dt>
            <dd data-testid="change" className="text-end text-4xl font-bold">
              <bdi dir="ltr">{formatDZD(frame.change, lang)}</bdi>
            </dd>
          </>
        ) : null}
      </dl>
      <p className="flex flex-wrap justify-center gap-x-10 gap-y-2 text-5xl font-bold">
        {thanks
          ? THANKS_LANGS.map((l) => (
              <span key={l} lang={l} dir={l === "ar" ? "rtl" : "ltr"}>
                {t("hardware:customerDisplay.thanks", { lng: l })}
              </span>
            ))
          : null}
      </p>
    </section>
  );
}

/** The current line, the weight while weighing (VRT-010), the saving (PRM-006), the shop's
 * BaridiMob QR while the customer pays by BaridiMob (HW-042), the amount due. */
function Sale({ frame, lang }: { frame: DisplayFrame; lang: Lang }) {
  const { t } = useTranslation();
  const money = (centimes: number) => formatDZD(centimes, lang);
  const { line, weight } = frame;
  return (
    <>
      <section className="flex flex-1 flex-col justify-center gap-4">
        {line ? (
          <div className="rounded-card border border-line bg-surface p-6">
            <p className="text-4xl font-bold">{line.name}</p>
            <p className="mt-2 text-end text-2xl text-ink-2 tabular-nums">
              <bdi dir="ltr">
                {formatQty(line.qty, lang)} × {money(line.unitPrice)} ={" "}
                <strong className="text-ink">{money(line.total)}</strong>
              </bdi>
            </p>
          </div>
        ) : null}
        {weight ? (
          <div
            data-testid="weight"
            className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-2 rounded-card bg-surface-2 p-6 text-3xl tabular-nums"
          >
            <span>{t("hardware:customerDisplay.weight")}</span>
            <span className="font-bold">
              {t("hardware:customerDisplay.kilos", { qty: formatQty(weight.net, lang) })}
            </span>
            {weight.unitPrice !== null ? (
              <span>{t("hardware:customerDisplay.perKilo", { price: money(weight.unitPrice) })}</span>
            ) : null}
            {weight.tare > 0 ? (
              <span className="text-xl text-ink-2">
                {t("hardware:customerDisplay.tare", { tare: formatQty(weight.tare, lang) })}
              </span>
            ) : null}
          </div>
        ) : null}
        {frame.saved > 0 ? (
          <p className="text-2xl font-bold text-ok">
            {t("hardware:customerDisplay.saved", { amount: money(frame.saved) })}
          </p>
        ) : null}
        {frame.payQr ? (
          <div className="flex flex-col items-center gap-2">
            <QrCode matrix={frame.payQr} size={260} label={t("hardware:customerDisplay.payBaridimob")} />
            <p className="text-2xl font-bold">{t("hardware:customerDisplay.payBaridimob")}</p>
          </div>
        ) : null}
      </section>
      <footer className="flex items-center justify-between gap-6 rounded-card bg-brand px-8 py-4 text-brand-ink">
        <span className="text-3xl font-bold">{t("hardware:customerDisplay.due")}</span>
        <span dir="ltr" data-testid="amount-due" className="text-[4rem] leading-none font-bold tabular-nums">
          {money(frame.total)}
        </span>
      </footer>
    </>
  );
}
