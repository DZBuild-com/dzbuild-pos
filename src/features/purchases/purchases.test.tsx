import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { InvoiceDialog } from "@/features/purchases/InvoiceDialog";
import { OrderEditor } from "@/features/purchases/OrderEditor";
import { PaySupplierDialog } from "@/features/purchases/PaySupplierDialog";
import { PurchasesPage } from "@/features/purchases/PurchasesPage";
import { ReceiveDialog } from "@/features/purchases/ReceiveDialog";
import { SupplierDetail } from "@/features/purchases/SupplierDetail";
import { SupplierReturnDialog } from "@/features/purchases/SupplierReturnDialog";
import { qk } from "@/lib/ipc/keys";
import type { Document, Session, SupplierHit } from "@/lib/ipc/types";
import { createQueryClient } from "@/lib/query";
import { documentFixture, productFixture, unitsFixture } from "@/test/fixtures-m1";
import {
  lotTrackedExtrasFixture,
  purchaseSummariesFixture,
  remainingFixture,
  statementFixture,
  supplierEntriesFixture,
  supplierEntriesPageFixture,
  supplierFixture,
} from "@/test/fixtures-m2";
import { managerSession } from "@/test/fixtures";
import { mockCommands, providers, renderWithProviders } from "@/test/render";
import { scanDigits } from "@/test/scan";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

const supplierHit: SupplierHit = {
  id: supplierFixture.id,
  code: supplierFixture.code,
  name: supplierFixture.name,
  phone: supplierFixture.phone,
  wilaya: supplierFixture.wilaya,
  balance: supplierFixture.balance,
  archived: false,
};

/** A role that keeps the supplier files and pays them, but does not see the purchases. */
const supplierKeeperSession: Session = { ...managerSession, permissions: ["suppliers.manage"] };

/** A carton of 12 Candia milks: its barcode brings the pack size. */
const cartonScan = { product: productFixture, packQty: 12_000 };

function typeCode(code: string) {
  const field = screen.getByLabelText("Ajouter un produit");
  fireEvent.change(field, { target: { value: code } });
  fireEvent.keyDown(field, { key: "Enter" });
}

describe("purchasing", () => {
  it("receives the remaining lines with a lot and shows the new CMUP from the backend", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      purchases_remaining: () => remainingFixture,
      products_extras_get: () => lotTrackedExtrasFixture,
      catalog_units_list: () => unitsFixture,
      purchases_receive: (args) => {
        sent.push(args.input);
        return {
          receipt: { id: "d-rc4", number: "RC2026-C1-000004" },
          costChanges: [{ productId: "p-lait", costBefore: 9_000, costAfter: 9_250 }],
          invoice: null,
          payment: null,
        };
      },
    });
    renderWithProviders(<ReceiveDialog supplier={supplierFixture} orderId="o-1" open onClose={() => {}} />);
    expect(await screen.findByText("Lait UHT 1 L")).toBeInTheDocument();
    fireEvent.change(await screen.findByLabelText("N° de lot — Lait UHT 1 L"), {
      target: { value: "L2609" },
    });
    fireEvent.change(screen.getByLabelText("Péremption — Lait UHT 1 L"), { target: { value: "2026-12-31" } });
    fireEvent.click(screen.getByRole("button", { name: "Valider la réception" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      supplierId: "s-cevital",
      orderId: "o-1",
      lines: [
        {
          productId: "p-lait",
          originLineId: "l-bc1-1",
          qty: 24_000,
          unitCost: 9_000,
          lotNumber: "L2609",
          expiryDate: "2026-12-31",
        },
      ],
      invoice: null,
    });
    expect(await screen.findByText(/92,50/)).toBeInTheDocument();
  });

  it("orders a carton by its barcode, saves the draft and validates it as a BC", async () => {
    const saved: unknown[] = [];
    const validated: unknown[] = [];
    const draft: Document = {
      ...documentFixture,
      id: "o-2",
      docType: "purchase_order",
      number: null,
      status: "draft",
      supplierId: supplierFixture.id,
      cashSessionId: null,
      subtotalHt: 108_000,
      taxTotal: 9_720,
      totalTtc: 117_720,
      validatedAt: null,
      lines: [],
      taxes: [],
      payments: [],
    };
    let current = draft;
    mockCommands({
      auth_session: () => managerSession,
      suppliers_search: () => [supplierHit],
      suppliers_get: () => supplierFixture,
      catalog_units_list: () => unitsFixture,
      products_find_by_code: () => cartonScan,
      purchases_save_order: (args) => {
        saved.push(args);
        return draft;
      },
      purchases_get: () => current,
      purchases_validate_order: (args) => {
        validated.push(args);
        current = { ...draft, status: "validated", number: "BC2026-C1-000002" };
        return current;
      },
      purchases_list: () => [],
      purchases_remaining: () => [],
    });
    renderWithProviders(
      <Routes>
        <Route path="/purchases/orders/:id" element={<OrderEditor />} />
      </Routes>,
      { route: "/purchases/orders/new?supplier=s-cevital" },
    );
    await screen.findByRole("heading", { name: /Nouvelle commande/ });
    typeCode("6130000000031");
    expect(await screen.findByLabelText("Unités par colis — Lait Candia 1 L")).toHaveValue("12");
    fireEvent.change(screen.getByLabelText("Coût du colis HT — Lait Candia 1 L"), {
      target: { value: "1080" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Valider la commande" }));
    await waitFor(() => expect(validated).toEqual([{ id: "o-2" }]));
    expect(saved[0]).toMatchObject({
      id: null,
      input: {
        supplierId: "s-cevital",
        lines: [{ productId: "p-milk", qty: 1_000, packQty: 12_000, unitCost: 108_000 }],
      },
    });
    expect(await screen.findByRole("heading", { name: /BC2026-C1-000002/ })).toBeInTheDocument();
  });

  it("adds a scanned carton while the focus is in a cost field", async () => {
    const codes: unknown[] = [];
    const sugar = { ...productFixture, id: "p-sugar", nameFr: "Sucre 1 kg" };
    mockCommands({
      auth_session: () => managerSession,
      suppliers_search: () => [supplierHit],
      suppliers_get: () => supplierFixture,
      catalog_units_list: () => unitsFixture,
      products_find_by_code: (args) => {
        codes.push(args.code);
        return args.code === "6130000000031" ? cartonScan : { product: sugar, packQty: 1_000 };
      },
      purchases_list: () => [],
      purchases_remaining: () => [],
    });
    renderWithProviders(
      <Routes>
        <Route path="/purchases/orders/:id" element={<OrderEditor />} />
      </Routes>,
      { route: "/purchases/orders/new?supplier=s-cevital" },
    );
    await screen.findByRole("heading", { name: /Nouvelle commande/ });
    typeCode("6130000000031");
    const cost = await screen.findByLabelText("Coût du colis HT — Lait Candia 1 L");
    cost.focus();
    // "scan, type the cost, scan the next carton": the scanner types into the cost field
    scanDigits(cost, "6130000000048");
    await waitFor(() => expect(codes).toEqual(["6130000000031", "6130000000048"]));
    expect(await screen.findByText("Sucre 1 kg")).toBeInTheDocument();
    expect(cost).toHaveValue("");
  });

  it("lists the orders with their receiving status and filters them by status", async () => {
    mockCommands({
      auth_session: () => managerSession,
      purchases_list: () => purchaseSummariesFixture.filter((d) => d.docType === "purchase_order"),
      suppliers_search: () => [supplierHit],
    });
    renderWithProviders(<PurchasesPage />, { route: "/purchases" });
    const table = await screen.findByRole("table", { name: "Bon de commande" });
    expect(await within(table).findByText("BC2026-C1-000001")).toBeInTheDocument();
    expect(within(table).getByText("Reçue en partie")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Excel (xlsx)" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("État"), { target: { value: "received" } });
    expect(await within(table).findByText(/Aucun document pour ces critères/)).toBeInTheDocument();
  });

  it("shows a duplicate supplier invoice number under the number it refuses", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      purchases_list: () => purchaseSummariesFixture.filter((d) => d.docType === "goods_receipt"),
      purchases_record_invoice: (args) => {
        sent.push(args.input);
        return Promise.reject({ code: "supplier_invoice_duplicate", field: "supplierNumber" });
      },
    });
    renderWithProviders(
      <InvoiceDialog supplier={supplierFixture} receiptIds={["d-rc1"]} open onClose={() => {}} />,
    );
    expect(await screen.findByText("RC2026-C1-000001")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("N° de la facture du fournisseur"), {
      target: { value: "F-2231" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer la facture" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      supplierId: "s-cevital",
      supplierNumber: "F-2231",
      receiptIds: ["d-rc1"],
      lines: [],
      payment: null,
    });
    expect(await screen.findByText(/porte déjà ce numéro/)).toBeInTheDocument();
    expect(screen.getByLabelText("N° de la facture du fournisseur")).toHaveAttribute("aria-invalid", "true");
  });

  it("pays a supplier in cash from the drawer", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      suppliers_open_items: () => [supplierEntriesFixture[0]],
      suppliers_pay: (args) => {
        sent.push(args.input);
        return { ...supplierEntriesFixture[1], amount: -214_000, balanceAfter: 286_000 };
      },
    });
    renderWithProviders(<PaySupplierDialog supplier={supplierFixture} open onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText("Montant payé"), { target: { value: "2140" } });
    fireEvent.click(
      await screen.findByRole("checkbox", { name: "Payé en espèces sorties du tiroir-caisse" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer le paiement" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      supplierId: "s-cevital",
      amount: 214_000,
      method: "cash",
      reference: null,
      fromTill: true,
      allocateTo: [],
    });
  });

  it("offers the drawer only to who may take cash out of it", async () => {
    mockCommands({ suppliers_open_items: () => [] });
    const client = createQueryClient();
    client.setQueryData(qk.session, {
      ...managerSession,
      permissions: managerSession.permissions.filter((p) => p !== "cash.paid_in_out"),
    });
    render(<PaySupplierDialog supplier={supplierFixture} open onClose={() => {}} />, {
      wrapper: providers(client),
    });
    expect(screen.getByRole("radio", { name: "Espèces" })).toHaveAttribute("aria-checked", "true");
    expect(screen.queryByRole("checkbox", { name: "Payé en espèces sorties du tiroir-caisse" })).toBeNull();
  });

  it("returns damaged goods to the supplier, credited on the account", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => managerSession,
      catalog_units_list: () => unitsFixture,
      products_find_by_code: () => ({ product: productFixture, packQty: 1_000 }),
      products_extras_get: () => ({ ...lotTrackedExtrasFixture, productId: "p-milk", trackLots: false }),
      purchases_return: (args) => {
        sent.push(args.input);
        return {
          document: { ...documentFixture, number: "BR2026-F-000001", totalTtc: 18_000 },
          entry: { ...supplierEntriesFixture[0], kind: "return", amount: -18_000, balanceAfter: 482_000 },
        };
      },
    });
    renderWithProviders(<SupplierReturnDialog supplier={supplierFixture} open onClose={() => {}} />);
    typeCode("6130000000017");
    fireEvent.change(await screen.findByLabelText("Quantité rendue — Lait Candia 1 L"), {
      target: { value: "2" },
    });
    fireEvent.change(screen.getByLabelText("Motif — Lait Candia 1 L"), { target: { value: "damaged" } });
    fireEvent.click(screen.getByRole("button", { name: "Valider le retour" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      supplierId: "s-cevital",
      creditSupplier: true,
      lines: [{ productId: "p-milk", qty: 2_000, unitCost: null, lotNumber: null, note: "Abîmé" }],
    });
    expect(await screen.findByRole("heading", { name: "Retour BR2026-F-000001 validé" })).toBeInTheDocument();
  });

  it("opens a supplier's file with the backend's balance, ageing and account entries", async () => {
    mockCommands({
      auth_session: () => managerSession,
      suppliers_get: () => supplierFixture,
      suppliers_entries: () => supplierEntriesPageFixture,
      suppliers_aging: () => [
        {
          partyId: "s-cevital",
          code: "F00001",
          name: "Cevital",
          phone: null,
          balance: 500_000,
          current: 300_000,
          days3160: 200_000,
          days6190: 0,
          over90: 0,
          advance: 0,
          oldestOpen: "2026-08-20",
        },
      ],
    });
    renderWithProviders(
      <Routes>
        <Route path="/purchases/suppliers/:id" element={<SupplierDetail />} />
      </Routes>,
      { route: "/purchases/suppliers/s-cevital" },
    );
    expect(await screen.findByRole("heading", { name: /Cevital/ })).toBeInTheDocument();
    // the ageing is asked once the session says `purchases.view`
    const days3160 = await screen.findByText("31 à 60 jours");
    await waitFor(() => expect(days3160.closest("dl")).toHaveTextContent(/2\s?000,00/));
    const entries = await screen.findByRole("table", { name: "Mouvements du compte" });
    // newest first, each with the balance after it
    const [, payment, invoice] = within(entries).getAllByRole("row");
    expect(payment).toHaveTextContent(/Chèque.*5\s?000,00/);
    expect(invoice).toHaveTextContent(/Facture fournisseur.*FF2026-C1-000001.*7\s?140,00/);
  });

  it("opens the supplier directory, not the purchases, to who keeps the suppliers only", async () => {
    const calls = mockCommands({
      auth_session: () => supplierKeeperSession,
      suppliers_search: () => [supplierHit],
    });
    renderWithProviders(<PurchasesPage />, { route: "/purchases?tab=orders" });
    // the button is the keeper's: the session has arrived
    expect(await screen.findByRole("button", { name: "Nouveau fournisseur" })).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Fournisseurs"]);
    expect(screen.getByRole("tab", { name: "Fournisseurs" })).toHaveAttribute("aria-selected", "true");
    const table = screen.getByRole("table", { name: "Fournisseurs" });
    expect(await within(table).findByText("Cevital")).toBeInTheDocument();
    expect(calls).not.toContain("purchases_list");
  });

  it("shows who keeps the suppliers only their file, without the purchase figures", async () => {
    const calls = mockCommands({
      auth_session: () => supplierKeeperSession,
      suppliers_get: () => supplierFixture,
      suppliers_entries: () => supplierEntriesPageFixture,
      suppliers_statement: () => ({ ...statementFixture, partyId: "s-cevital" }),
    });
    renderWithProviders(
      <Routes>
        <Route path="/purchases/suppliers/:id" element={<SupplierDetail />} />
      </Routes>,
      { route: "/purchases/suppliers/s-cevital" },
    );
    expect(await screen.findByRole("button", { name: "Payer le fournisseur" })).toBeInTheDocument();
    expect(screen.getByText("Reste dû au fournisseur").closest("dl")).toHaveTextContent(/5\s?000,00/);
    // the ageing and the documents are purchases (`purchases.view`): not shown as zeros or empty
    expect(screen.queryByText("31 à 60 jours")).toBeNull();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "Mouvements du compte",
      "Paiements",
      "Relevé de compte",
    ]);
    fireEvent.click(screen.getByRole("tab", { name: "Relevé de compte" }));
    expect(await screen.findByRole("table", { name: "Relevé de compte" })).toBeInTheDocument();
    // the statement's export is the purchases report, which the backend refuses without it
    expect(screen.queryByRole("button", { name: "Excel (xlsx)" })).toBeNull();
    expect(calls).not.toContain("suppliers_aging");
    expect(calls).not.toContain("purchases_list");
  });
});
