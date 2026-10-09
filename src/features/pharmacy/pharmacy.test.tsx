import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { missingKeys, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import {
  chifaCheck,
  chifaFile,
  chifaPreview,
  expiryBoard,
  packState,
  registerEntry,
  sessionWith,
} from "@/test/fixtures-m8";
import { ChifaImport } from "./ChifaImport";
import { ControlledRegister } from "./ControlledRegister";
import { ProductsScreen } from "@/features/products/ProductsScreen";
import { ExpiryBoard } from "./ExpiryBoard";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0x0, so every element measures 1024x600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});
// unmount first: the screens' event listeners unregister through the mocked IPC
afterEach(() => {
  cleanup();
  clearMocks();
});
// The lots' value at cost shows to whoever sees costs (the backend sends zero to the others).
const pharmacist = sessionWith([
  "products.view",
  "products.edit",
  "stock.view",
  "pharmacy.register",
  "reports.profit",
]);

describe("pharmacy screens", () => {
  it("lists expired lots before the ones expiring soon, with the cost at risk", async () => {
    mockCommands({
      auth_session: () => pharmacist,
      packs_state: () => packState({ modules: ["pharmacy", "lots"] }),
      pharmacy_expiry_board: () => expiryBoard(),
    });
    await renderM8(<ExpiryBoard />, { session: pharmacist });
    const tables = await screen.findAllByRole("table");
    expect(within(tables[0]!).getByText("Amoxicilline 500 mg")).toBeInTheDocument();
    expect(screen.getByText("Valeur au coût : 12 450,00 DA")).toBeInTheDocument();
  });

  it("refuses to apply a Chifa list while a row has a problem", async () => {
    mockCommands({
      auth_session: () => pharmacist,
      packs_state: () => packState({ modules: ["pharmacy"] }),
      imports_preview: () => chifaPreview(),
      chifa_check: () => chifaCheck({ problems: 1 }),
    });
    const user = userEvent.setup();
    // the webview holds the staged file's token, never its path (M2's import rule)
    await renderM8(<ChifaImport initialFile={chifaFile()} />, { session: pharmacist });
    await user.click(await screen.findByRole("button", { name: "Vérifier" }));
    expect(
      await screen.findByText("Ligne 14, colonne Tarif de référence : montant illisible"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Appliquer" })).toBeDisabled();
  });

  it("shows the controlled register only to its holders and prints it", async () => {
    const calls = mockCommands({
      auth_session: () => pharmacist,
      packs_state: () => packState({ modules: ["pharmacy"] }),
      controlled_register_entries: () => [registerEntry()],
      exports_pdf: () => ({ path: "C:/Users/pharmacien/Documents/registre.pdf", rows: 1 }),
    });
    const user = userEvent.setup();
    await renderM8(<ControlledRegister />, { session: pharmacist });
    expect(await screen.findByText("Dr Benali")).toBeInTheDocument();
    expect(screen.getByText("Données de santé : accès réservé")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Imprimer le registre" }));
    await waitFor(() => expect(calls).toContain("exports_pdf"));
  });

  it("shows the board and the register in Arabic with every text translated", async () => {
    const arabic = sessionWith(["stock.view", "pharmacy.register", "reports.profit"], "ar");
    mockCommands({
      packs_state: () => packState({ modules: ["pharmacy", "lots"] }),
      pharmacy_expiry_board: () => expiryBoard(),
      controlled_register_entries: () => [registerEntry()],
    });
    missingKeys.clear();
    await renderM8(
      <>
        <ExpiryBoard />
        <ControlledRegister />
      </>,
      { lang: "ar", session: arabic },
    );
    expect(await screen.findByText("أموكسيسيلين 500 ملغ")).toBeInTheDocument();
    expect(await screen.findByText("Dr Benali")).toBeInTheDocument();
    expect(screen.getByText("معطيات صحية: الاطلاع محصور")).toBeInTheDocument();
    expect([...missingKeys]).toEqual([]);
  });

  it("opens the pharmacy's pages of the products screen while its module is on", async () => {
    mockCommands({
      packs_state: () => packState({ modules: ["pharmacy"] }),
      controlled_register_entries: () => [registerEntry()],
    });
    await renderM8(<ProductsScreen session={pharmacist} />, {
      session: pharmacist,
      route: "/products?tab=register",
    });
    expect(await screen.findByRole("tab", { name: "Registre", selected: true })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Liste Chifa" })).toBeInTheDocument();
    expect(await screen.findByText("Dr Benali")).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Prix plafonnés" })).not.toBeInTheDocument();
  });
});
