import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PromotionForm } from "@/features/promotions/PromotionForm";
import { PromotionsPage } from "@/features/promotions/PromotionsPage";
import type { Promotion } from "@/lib/ipc/types";
import { categoriesFixture } from "@/test/fixtures-m1";
import { promotionFixture } from "@/test/fixtures-m2";
import { managerSession, settingsValues } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

/** Autumn soldes declared outside the wali's winter window. */
const soldeFixture: Promotion = {
  ...promotionFixture,
  id: "promo-2",
  nameAr: "تخفيضات الخريف",
  nameFr: "Soldes d'automne",
  nameEn: "Autumn sale",
  campaign: "solde",
  isLegalSoldes: true,
  triggerMode: "auto",
  value: 3_000,
  startsOn: "2026-10-01",
  endsOn: "2026-10-15",
  outsideSoldesWindow: true,
  products: 5,
};

const windows = [{ from: "2026-01-15", to: "2026-02-15" }];

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual shows no row in a 0×0 scroller.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("promotions", () => {
  it("saves a soldes campaign with its dates and warns outside the legal windows", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      settings_get: (args) => (args.key === "promotions.soldes_windows" ? windows : null),
      promotions_create: (args) => {
        saved.push(args.input);
        return { id: "promo-1", outsideSoldesWindow: true };
      },
    });
    renderWithProviders(<PromotionForm onSaved={() => {}} />);
    fireEvent.change(screen.getByLabelText("Nom (français)"), { target: { value: "Soldes d'automne" } });
    fireEvent.change(screen.getByLabelText("Campagne"), { target: { value: "solde" } });
    fireEvent.change(screen.getByLabelText("Réduction (%)"), { target: { value: "30" } });
    fireEvent.change(screen.getByLabelText("Du"), { target: { value: "2026-10-01" } });
    fireEvent.change(screen.getByLabelText("Au"), { target: { value: "2026-10-15" } });
    expect(screen.getByRole("alert")).toHaveTextContent(/hors des périodes de soldes/);
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      kind: "percent",
      campaign: "solde",
      value: 3000,
      startsOn: "2026-10-01",
      endsOn: "2026-10-15",
    });
  });

  it("says nothing inside a declared window and asks a legal campaign for both dates", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      settings_get: (args) => (args.key === "promotions.soldes_windows" ? windows : null),
      promotions_create: (args) => {
        saved.push(args.input);
        return soldeFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PromotionForm onSaved={() => {}} />);
    await user.type(screen.getByLabelText("Nom (français)"), "Soldes d'hiver");
    await user.selectOptions(screen.getByLabelText("Campagne"), "solde");
    await user.type(screen.getByLabelText("Réduction (%)"), "20");
    fireEvent.change(screen.getByLabelText("Du"), { target: { value: "2026-01-20" } });
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    expect(
      await screen.findByText(
        "Des soldes, une liquidation ou un déballage ont une date de début et une date de fin. Saisissez les deux.",
      ),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Au"), { target: { value: "2026-02-10" } });
    await waitFor(() => expect(screen.queryByText(/hors des périodes de soldes/)).toBeNull());
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({ triggerMode: "auto", scope: { kind: "all" }, value: 2_000 });
  });

  it("offers the named discount at the till for a percent promotion only, and sends buy X get Y in pieces", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      settings_get: (args) => settingsValues[String(args.key)] ?? null,
      catalog_categories_list: () => categoriesFixture,
      promotions_create: (args) => {
        saved.push(args.input);
        return promotionFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PromotionForm onSaved={() => {}} />);
    const trigger = screen.getByLabelText("Déclenchement");
    expect(within(trigger).getByRole("option", { name: /Remise nommée/ })).toBeEnabled();
    await user.type(screen.getByLabelText("Nom (français)"), "2 achetés, 1 offert");
    await user.selectOptions(screen.getByLabelText("Type de promotion"), "buy_x_get_y");
    expect(within(trigger).getByRole("option", { name: /Remise nommée/ })).toBeDisabled();
    await user.type(screen.getByLabelText("Articles achetés"), "2");
    await user.type(screen.getByLabelText("Articles offerts"), "1");
    await user.selectOptions(screen.getByLabelText("S'applique à"), "categories");
    await user.click(await screen.findByRole("checkbox", { name: "Crèmerie" }));
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      kind: "buy_x_get_y",
      campaign: "promotion",
      triggerMode: "auto",
      scope: { kind: "categories", ids: ["c-dairy"] },
      value: 0,
      buyQty: 2_000,
      getQty: 1_000,
      startsOn: null,
      endsOn: null,
    });
  });

  it("lists the campaigns, flags soldes outside the windows and exports their declaration", async () => {
    const exported: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      settings_get: (args) => (args.key === "promotions.soldes_windows" ? windows : null),
      promotions_list: () => [promotionFixture, soldeFixture],
      exports_save: (args) => {
        exported.push(args.request);
        return { path: "C:\\declaration.xlsx", rows: 5 };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PromotionsPage />, { route: "/products/promotions" });
    const row = await screen.findByRole("row", { name: /Soldes d'automne/ });
    expect(within(row).getByText("Soldes")).toBeInTheDocument();
    expect(within(row).getByText("Hors des périodes de soldes")).toBeInTheDocument();
    expect(screen.getByRole("row", { name: /Remise fidèle/ })).toHaveTextContent("Remise nommée");
    await user.click(row);
    const dialog = await screen.findByRole("dialog", { name: "Soldes d'automne" });
    await user.click(within(dialog).getByRole("button", { name: "Excel (xlsx)" }));
    await waitFor(() => expect(exported).toHaveLength(1));
    expect(exported[0]).toEqual({
      target: {
        kind: "report",
        request: {
          kind: "promotion_declaration",
          from: "2026-10-01",
          to: "2026-10-15",
          promotionId: "promo-2",
        },
      },
      format: "xlsx",
      lang: "fr",
    });
  });
});
