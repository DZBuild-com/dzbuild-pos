import { act, render, screen } from "@testing-library/react";
import { useTranslation } from "react-i18next";
import { describe, expect, it } from "vitest";
import { setLanguage } from "@/lib/i18n";

function Probe() {
  const { t } = useTranslation();
  return <p>{t("common:actions.save")}</p>;
}

describe("runtime language switch", () => {
  it("sets dir=rtl and lang=ar for Arabic, back to ltr for French", async () => {
    await act(async () => {
      await setLanguage("ar");
    });
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    expect(document.documentElement).toHaveAttribute("lang", "ar");
    await act(async () => {
      await setLanguage("fr");
    });
    expect(document.documentElement).toHaveAttribute("dir", "ltr");
    expect(document.documentElement).toHaveAttribute("lang", "fr");
  });

  it("re-renders mounted components without a reload", async () => {
    render(<Probe />);
    expect(screen.getByText("Enregistrer")).toBeInTheDocument();
    await act(async () => {
      await setLanguage("ar");
    });
    expect(screen.getByText("حفظ")).toBeInTheDocument();
    await act(async () => {
      await setLanguage("en");
    });
    expect(screen.getByText("Save")).toBeInTheDocument();
  });
});
