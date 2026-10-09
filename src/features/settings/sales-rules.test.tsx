import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { SalesRulesSettings } from "@/features/settings/SalesRulesSettings";
import { allowedSections } from "@/features/settings/sections";
import { cashierSession, ownerSession, settingsValues } from "@/test/fixtures";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";

function mockSettings(saved: unknown[]) {
  mockCommands({
    settings_get: ({ key }) => settingsValues[String(key)],
    settings_set: (args) => {
      saved.push(args);
      return args.value;
    },
  });
}

describe("Settings › Sales and cash", () => {
  it("is read-only on a till, with the main PC's name and no save button", async () => {
    mockCommands({
      settings_get: ({ key }) => settingsValues[String(key)],
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<SalesRulesSettings />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Monnaie maximale")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Enregistrer les règles" })).not.toBeInTheDocument();
  });

  it("saves only the rules that changed, amounts in centimes", async () => {
    const saved: unknown[] = [];
    mockSettings(saved);
    const user = userEvent.setup();
    renderWithProviders(<SalesRulesSettings />);
    const maxChange = await screen.findByLabelText("Monnaie maximale");
    await user.clear(maxChange);
    await user.type(maxChange, "2 000");
    await user.selectOptions(screen.getByLabelText("Lots périmés"), "Bloquer");
    await user.click(screen.getByRole("button", { name: "Enregistrer les règles" }));
    await waitFor(() => expect(saved).toHaveLength(2));
    expect(saved).toEqual(
      expect.arrayContaining([
        { key: "sales.max_change", value: 200_000 },
        { key: "stock.expired_lots", value: "block" },
      ]),
    );
  });

  it("adds a soldes period and refuses one that ends before it starts", async () => {
    const saved: unknown[] = [];
    mockSettings(saved);
    const user = userEvent.setup();
    renderWithProviders(<SalesRulesSettings />);
    await user.click(await screen.findByRole("button", { name: "Ajouter une période" }));
    fireEvent.change(screen.getByLabelText("Du"), { target: { value: "2027-02-20" } });
    fireEvent.change(screen.getByLabelText("Au"), { target: { value: "2027-01-10" } });
    await user.click(screen.getByRole("button", { name: "Enregistrer les règles" }));
    expect(
      await screen.findByText("La date de fin ne peut pas précéder la date de début."),
    ).toBeInTheDocument();
    expect(saved).toHaveLength(0);
    fireEvent.change(screen.getByLabelText("Du"), { target: { value: "2027-01-10" } });
    fireEvent.change(screen.getByLabelText("Au"), { target: { value: "2027-02-20" } });
    await user.click(screen.getByRole("button", { name: "Enregistrer les règles" }));
    await waitFor(() =>
      expect(saved).toEqual([
        { key: "promotions.soldes_windows", value: [{ from: "2027-01-10", to: "2027-02-20" }] },
      ]),
    );
  });

  it("is a section of the owner's settings, hidden from a cashier", () => {
    expect(allowedSections(ownerSession).map((s) => s.path)).toContain("sales");
    expect(allowedSections(cashierSession).map((s) => s.path)).not.toContain("sales");
  });
});
