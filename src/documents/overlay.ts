import type { Lang } from "@/lib/i18n";
import type { DocumentLayout, DocumentPrintModel, DocumentTexts, Trilingual } from "@/lib/ipc/types";

/** pos-core `Trilingual::in_lang`: the text in `lang`, else in French, Arabic, English, as the
 * print model picks it; `null` when all are blank. */
const FALLBACK: Record<Lang, readonly Lang[]> = {
  ar: ["ar", "fr", "ar", "en"],
  fr: ["fr", "fr", "ar", "en"],
  en: ["en", "fr", "ar", "en"],
};

const text = (value: Trilingual | undefined, lang: Lang): string | null =>
  FALLBACK[lang].map((l) => value?.[l]?.trim() ?? "").find((t) => t !== "") ?? null;

/** The settings preview: the print fixture with the settings being edited laid over it. */
export function overlay(
  model: DocumentPrintModel,
  change: { layout?: DocumentLayout; texts?: DocumentTexts; activity?: Trilingual; lang: Lang },
): DocumentPrintModel {
  const { layout, texts, activity, lang } = change;
  return {
    ...model,
    layout: layout ?? model.layout,
    headerText: texts ? text(texts.header, lang) : model.headerText,
    footerText: texts ? text(texts.footer, lang) : model.footerText,
    termsText: texts ? text(texts.terms, lang) : model.termsText,
    seller: activity ? { ...model.seller, activity: text(activity, lang) } : model.seller,
  };
}
