import { describe, expect, it } from "vitest";
import { BACK_OFFICE_NAMESPACES, i18n, LANGS, loadLanguage, NAMESPACES, setLanguage } from "@/lib/i18n";
import arOffice from "@/locales/bundles/ar-office";
import arTill from "@/locales/bundles/ar-till";
import enOffice from "@/locales/bundles/en-office";
import enTill from "@/locales/bundles/en-till";
import frOffice from "@/locales/bundles/fr-office";
import frTill from "@/locales/bundles/fr-till";

const namespaces = (bundle: Record<string, unknown>) =>
  Object.keys(bundle)
    .map((file) => file.slice(file.lastIndexOf("/") + 1, -".json".length))
    .sort();

// NFR-010 (finding 28): each language is two chunks inside the app (offline): the till's texts,
// read before the first paint in that language, and the back office's, read when a back-office
// screen first opens. The tests load the back office's once (`src/test/setup.ts`).
describe("languages loaded on first use", () => {
  it("splits every language into the till's texts and the back office's, with nothing left out", () => {
    const bundles = { ar: [arTill, arOffice], fr: [frTill, frOffice], en: [enTill, enOffice] } as const;
    for (const lang of LANGS) {
      const [till, office] = bundles[lang];
      expect(namespaces(office), lang).toEqual([...BACK_OFFICE_NAMESPACES].sort());
      expect(
        namespaces(till).some((ns) => namespaces(office).includes(ns)),
        lang,
      ).toBe(false);
      expect([...namespaces(till), ...namespaces(office)].sort(), lang).toEqual([...NAMESPACES].sort());
    }
  });

  it("keeps the language asked for last while an earlier one is still loading", async () => {
    expect(i18n.hasResourceBundle("ar", "common")).toBe(false);
    const toArabic = setLanguage("ar");
    await setLanguage("fr");
    await toArabic;
    expect(i18n.language).toBe("fr");
    expect(document.documentElement.dir).toBe("ltr");
  });

  it("brings Arabic and English in before the switch, once", async () => {
    expect(i18n.hasResourceBundle("fr", "pos")).toBe(true);
    expect(i18n.hasResourceBundle("en", "common")).toBe(false);
    await setLanguage("en");
    expect(i18n.t("common:actions.save")).toBe("Save");
    expect(i18n.hasResourceBundle("en", "settings")).toBe(true);
    expect(i18n.hasResourceBundle("en", "products")).toBe(true);
    const first = loadLanguage("ar");
    expect(loadLanguage("ar")).toBe(first);
    await first;
    expect(i18n.t("common:actions.save", { lng: "ar" })).toBe("حفظ");
    await setLanguage("fr");
  });
});
