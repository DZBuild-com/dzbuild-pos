import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CustomerDetail } from "@/features/customers/CustomerDetail";
import { CustomerForm } from "@/features/customers/CustomerForm";
import { VersementDialog } from "@/features/customers/VersementDialog";
import { CustomersPage } from "@/features/customers/CustomersPage";
import {
  collectQuoteFixture,
  customerFixture,
  customerHitsFixture,
  ledgerPageFixture,
  statementFixture,
} from "@/test/fixtures-m2";
import { managerSession, cashierSession, packFixture } from "@/test/fixtures";
import { linkedStatus } from "@/test/fixtures-m7";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("customers", () => {
  it("lists customers with their balance and hides the export from a cashier", async () => {
    mockCommands({ auth_session: () => cashierSession, customers_search: () => customerHitsFixture });
    renderWithProviders(<CustomersPage />);
    expect(await screen.findByText("Karim Benali")).toBeInTheDocument();
    expect(screen.getByText(/3\s?000,00/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Excel (xlsx)" })).not.toBeInTheDocument();
  });

  it("shows the stamp duty from the backend and collects cash plus duty", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      credit_quote: () => collectQuoteFixture,
      credit_collect: (args) => {
        sent.push(args.request);
        return {
          collected: {
            entry: { receiptNumber: "RV2026-000001" },
            quote: collectQuoteFixture,
            customer: customerFixture,
            allocations: [],
          },
          printQueued: true,
        };
      },
    });
    renderWithProviders(<VersementDialog customer={customerFixture} open onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText("Montant du versement"), { target: { value: "٢٠٠٠" } });
    expect(await screen.findByText(/Droit de timbre/)).toBeInTheDocument();
    expect(screen.getByText(/2\s?020,00/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ input: { amount: 200_000, method: "cash" }, printReceipt: true });
  });

  it("shows the backend's refusal under a reseller's NIF and sends the phone in Western digits", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      datapack_get: () => packFixture,
      catalog_price_levels_list: () => [],
      customers_create: (args) => {
        sent.push(args.input);
        return Promise.reject({ code: "required", field: "nif" });
      },
    });
    renderWithProviders(<CustomerForm customer={null} onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText("Nom"), { target: { value: "Rachid Belkacem" } });
    fireEvent.change(screen.getByLabelText("Type de client"), { target: { value: "reseller" } });
    expect(await screen.findByText(/Obligatoire pour un revendeur/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/^Téléphone/), { target: { value: "٠٥٥٠ ١٢ ٣٤ ٥٦" } });
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer le client" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      name: "Rachid Belkacem",
      customerType: "reseller",
      phone: "0550 12 34 56",
    });
    expect(await screen.findByText("Remplissez ce champ.")).toBeInTheDocument();
    expect(screen.getByLabelText("NIF")).toHaveAttribute("aria-invalid", "true");
  });

  function openFile(extra: Record<string, Handler>) {
    mockCommands({
      auth_session: () => managerSession,
      customers_get: () => customerFixture,
      credit_entries: () => ledgerPageFixture,
      ...extra,
    });
    renderWithProviders(
      <Routes>
        <Route path="/customers/:id" element={<CustomerDetail />} />
      </Routes>,
      { route: "/customers/c-karim" },
    );
  }

  it("opens a customer's file with the backend's figures and reprints a versement receipt", async () => {
    const reprinted: unknown[] = [];
    openFile({
      credit_reprint_receipt: (args) => {
        reprinted.push(args);
        return { printer: "Caisse 1", bytes: 800, raster: false };
      },
    });
    expect(await screen.findByRole("heading", { name: /Karim Benali/ })).toBeInTheDocument();
    expect((await screen.findByText("Crédit disponible")).closest("dl")).toHaveTextContent(/2\s?000,00/);
    const entries = await screen.findByRole("table", { name: "Mouvements du compte" });
    // newest first, each with the balance after it
    const [, versement, sale] = within(entries).getAllByRole("row");
    expect(versement).toHaveTextContent(/Versement · Espèces.*RV2026-000001.*3\s?000,00/);
    expect(sale).toHaveTextContent(/Vente à crédit.*TK2026-C1-000012.*5\s?000,00/);
    fireEvent.click(within(entries).getByRole("button", { name: "Réimprimer RV2026-000001" }));
    await waitFor(() => expect(reprinted).toEqual([{ entryId: "e-versement", lang: null }]));
  });

  it("warns on the customer's file when DZBuild flags his number (DZB-015)", async () => {
    openFile({
      cloud_status: () => linkedStatus,
      cloud_customer_flag: () => ({ isBanned: true, fraudScore: 95 }),
    });
    expect(await screen.findByRole("heading", { name: /Karim Benali/ })).toBeInTheDocument();
    expect(await screen.findByRole("status", { name: "Acheteur signalé" })).toHaveTextContent(
      "DZBuild a signalé ce numéro.",
    );
  });

  it("shows the newest entries first and pages back to older ones on demand", async () => {
    const asked: unknown[] = [];
    const [versement, sale] = ledgerPageFixture.entries;
    openFile({
      credit_entries: (args) => {
        const query = args.query as { before?: string | null; limit: number };
        asked.push(query);
        // the opening balance is on the older page: the backend says it is set whatever the page
        return query.before
          ? { entries: [sale], truncated: false, hasOpening: true }
          : { entries: [versement], truncated: true, hasOpening: true };
      },
    });
    const entries = await screen.findByRole("table", { name: "Mouvements du compte" });
    expect(await within(entries).findByText(/RV2026-000001/)).toBeInTheDocument();
    expect(within(entries).queryByText(/TK2026-C1-000012/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Solde d'ouverture" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Charger plus" }));
    expect(await within(entries).findByText(/TK2026-C1-000012/)).toBeInTheDocument();
    expect(asked).toEqual([
      { from: null, to: null, limit: 500, before: null },
      { from: null, to: null, limit: 500, before: "e-versement" },
    ]);
    expect(screen.queryByRole("button", { name: "Charger plus" })).not.toBeInTheDocument();
  });

  it("prints the statement of the chosen period in the chosen language", async () => {
    const printed: unknown[] = [];
    openFile({
      credit_statement: () => statementFixture,
      credit_print_statement: (args) => {
        printed.push(args);
        return { printer: "Caisse 1", bytes: 1200, raster: true };
      },
    });
    fireEvent.click(await screen.findByRole("tab", { name: "Relevé de compte" }));
    fireEvent.change(screen.getByLabelText("Du"), { target: { value: "2026-09-01" } });
    fireEvent.change(screen.getByLabelText("Au"), { target: { value: "2026-09-30" } });
    expect(await screen.findByText("Solde à la fin")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: "العربية" }));
    fireEvent.click(screen.getByRole("button", { name: "Imprimer le relevé" }));
    await waitFor(() =>
      expect(printed).toEqual([{ customerId: "c-karim", from: "2026-09-01", to: "2026-09-30", lang: "ar" }]),
    );
  });
});
