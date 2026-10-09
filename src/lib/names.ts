import type { Lang } from "@/lib/i18n";

export interface Trilingual {
  nameAr: string;
  nameFr: string;
  nameEn: string;
}

const ORDER: Record<Lang, (keyof Trilingual)[]> = {
  ar: ["nameAr", "nameFr", "nameEn"],
  fr: ["nameFr", "nameAr", "nameEn"],
  en: ["nameEn", "nameFr", "nameAr"],
};

/** Name in the UI language, falling back fr → ar → en (spec §5). */
export function localizedName(item: Trilingual, lang: Lang): string {
  for (const key of ORDER[lang]) {
    const value = item[key].trim();
    if (value) return value;
  }
  return "";
}
