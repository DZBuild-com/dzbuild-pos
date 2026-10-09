import { describe, expect, it } from "vitest";
import type { DocumentPrintModel } from "@/lib/ipc/types";
import invoice from "./__fixtures__/invoice.fr.json";
import { overlay } from "./overlay";

const base = invoice as unknown as DocumentPrintModel;

describe("settings preview overlay", () => {
  it("applies the layout, the texts and the activity of the chosen language", () => {
    const out = overlay(base, {
      lang: "fr",
      layout: { ...base.layout, accent: "ink", paper: "a5", showLogo: false },
      texts: {
        header: { ar: "", fr: "Livraison gratuite à Alger", en: "" },
        footer: { ar: "", fr: "Merci de votre confiance", en: "" },
        terms: { ar: "", fr: "Paiement à 30 jours", en: "" },
        warranty: { ar: "", fr: "", en: "" },
      },
      activity: { ar: "", fr: "Quincaillerie générale", en: "" },
    });
    expect(out.layout).toMatchObject({ accent: "ink", paper: "a5", showLogo: false });
    expect(out.headerText).toBe("Livraison gratuite à Alger");
    expect(out.footerText).toBe("Merci de votre confiance");
    expect(out.termsText).toBe("Paiement à 30 jours");
    expect(out.seller.activity).toBe("Quincaillerie générale");
    expect(out.number).toBe(base.number);
    expect(base.layout.accent).toBe("brand");
  });

  it("drops an empty text instead of printing a blank line", () => {
    const out = overlay(base, {
      lang: "fr",
      texts: {
        header: { ar: "", fr: "  ", en: "" },
        footer: { ar: "", fr: "", en: "" },
        terms: { ar: "", fr: "", en: "" },
        warranty: { ar: "", fr: "", en: "" },
      },
    });
    expect(out.headerText).toBeNull();
  });

  it("falls back to the French text as the printed document does", () => {
    const out = overlay(base, {
      lang: "ar",
      texts: {
        header: { ar: "", fr: "", en: "" },
        footer: { ar: "", fr: "Merci de votre confiance", en: "Thank you" },
        terms: { ar: "الدفع عند الاستلام", fr: "Paiement à la livraison", en: "" },
        warranty: { ar: "", fr: "", en: "" },
      },
      activity: { ar: "", fr: "", en: "Hardware" },
    });
    expect(out.footerText).toBe("Merci de votre confiance");
    expect(out.termsText).toBe("الدفع عند الاستلام");
    expect(out.seller.activity).toBe("Hardware");
  });
});
