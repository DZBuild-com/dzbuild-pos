import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { packState, sessionWith, zakatPreview } from "@/test/fixtures-m8";
import { ZakatScreen } from "./ZakatScreen";

// The setup file clears the IPC mocks after unmounting: clearing them first would break the LAN
// status listener's unlisten on unmount (an unhandled rejection).

const owner = sessionWith(["reports.profit", "settings.company"]);

/** The zakat backend: the nisab settings the shop entered (`null` = none yet), the preview
 * refused with `zakat_nisab_missing` until a nisab is stored, every `settings_set` logged. */
function backend(stored: { nisab: number | null; source: string }, session = owner) {
  const settings: Record<string, unknown> = {
    "zakat.nisab": stored.nisab,
    "zakat.nisab_source": stored.source,
  };
  const saved: Array<{ key: unknown; value: unknown }> = [];
  const calls = mockCommands({
    auth_session: () => session,
    packs_state: () => packState({ modules: ["zakat"] }),
    settings_get: ({ key }) => settings[String(key)] ?? null,
    settings_set: ({ key, value }) => {
      saved.push({ key, value });
      settings[String(key)] = value;
      return value;
    },
    zakat_preview: () => {
      const nisab = settings["zakat.nisab"];
      if (typeof nisab !== "number") return Promise.reject({ code: "zakat_nisab_missing" });
      return zakatPreview({ nisab, nisabSource: String(settings["zakat.nisab_source"]) });
    },
    zakat_list: () => [],
  });
  return { saved, calls };
}

it("shows the base against the nisab, the Hijri date and the disclaimer, in Arabic", async () => {
  backend({ nisab: 229_500_000, source: "MARW, 1447" }, sessionWith(["reports.profit"], "ar"));
  await renderM8(<ZakatScreen />, { lang: "ar" });
  expect(await screen.findByText("17 ربيع الآخر 1448")).toBeInTheDocument();
  expect(screen.getByText("الزكاة المستحقة")).toBeInTheDocument();
  expect(screen.getByText(/وليس فتوى/)).toBeInTheDocument();
  // the source is the shop's own note, never the data pack's
  expect(screen.getByText("المصدر: MARW, 1447")).toBeInTheDocument();
  // without settings.company the nisab is shown, not edited
  expect(screen.getByLabelText("النصاب (دج)")).toBeDisabled();
  expect(screen.queryByRole("button", { name: "حفظ النصاب" })).toBeNull();
});

describe("the nisab of the year", () => {
  it("asks for the nisab first, instead of an error, while the shop has entered none", async () => {
    backend({ nisab: null, source: "" });
    await renderM8(<ZakatScreen />);
    expect(await screen.findByRole("heading", { name: "Saisissez d'abord le nissab" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Pas encore saisi.")).toBeInTheDocument();
    expect(screen.getByLabelText("Nissab (DA)")).toHaveValue("");
  });

  it("saves the typed nisab in centimes with its source, and the calculation follows", async () => {
    const { saved, calls } = backend({ nisab: null, source: "" });
    const user = userEvent.setup();
    await renderM8(<ZakatScreen />);
    await screen.findByRole("heading", { name: "Saisissez d'abord le nissab" });
    const previews = calls.filter((c) => c === "zakat_preview").length;
    await user.type(screen.getByLabelText("Nissab (DA)"), "2295000");
    await user.type(screen.getByLabelText(/^Source/), "  MARW, 1447 ");
    await user.click(screen.getByRole("button", { name: "Enregistrer le nissab" }));
    await waitFor(() =>
      expect(saved).toEqual([
        { key: "zakat.nisab", value: 229_500_000 },
        { key: "zakat.nisab_source", value: "MARW, 1447" },
      ]),
    );
    expect(await screen.findByText("Source : MARW, 1447")).toBeInTheDocument();
    expect(calls.filter((c) => c === "zakat_preview").length).toBeGreaterThan(previews);
    expect(screen.queryByRole("heading", { name: "Saisissez d'abord le nissab" })).toBeNull();
  });

  it("refuses an empty or zero nisab without writing anything", async () => {
    const { saved } = backend({ nisab: null, source: "" });
    const user = userEvent.setup();
    await renderM8(<ZakatScreen />);
    await screen.findByRole("heading", { name: "Saisissez d'abord le nissab" });
    await user.click(screen.getByRole("button", { name: "Enregistrer le nissab" }));
    expect(await screen.findByLabelText("Nissab (DA)")).toHaveAttribute("aria-invalid", "true");
    await user.type(screen.getByLabelText("Nissab (DA)"), "0");
    await user.click(screen.getByRole("button", { name: "Enregistrer le nissab" }));
    expect(saved).toEqual([]);
  });

  it("says the shop entered the nisab when it noted no source", async () => {
    backend({ nisab: 229_500_000, source: "" });
    await renderM8(<ZakatScreen />);
    expect(await screen.findByText("Source : saisi par la boutique")).toBeInTheDocument();
    expect(screen.getByLabelText("Nissab (DA)")).not.toHaveValue("");
  });
});
