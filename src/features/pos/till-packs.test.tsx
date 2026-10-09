import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cashierTill,
  departmentKeys,
  topUpOperators,
  keyMap,
  packState,
  phoneExtras,
  phoneScanHit,
  serialsInStock,
  variantGrid,
} from "@/test/fixtures-m8";
import { customerFixture } from "@/test/fixtures-m2";
import { type Handler, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { useCart } from "./cart-store";
import { useExtras } from "./extras-store";
import { SaleScreen } from "./SaleScreen";

beforeEach(() => {
  // jsdom has no layout: M1b's virtualised grid renders its tiles only with a measured scroller
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
});

afterEach(() => {
  useCart.getState().clear("fr");
  useExtras.getState().clear();
  useExtras.getState().setVan(null);
  // this PC remembers the department keys in place of the grid
  window.localStorage.removeItem("dzpos.till.departments");
});

function till(modules: string[], extra: Record<string, Handler> = {}) {
  return mockCommands({
    ...cashierTill(),
    packs_state: () => packState({ modules: modules as never }),
    till_key_map: () => keyMap(),
    ...extra,
  });
}

describe("till pack extensions", () => {
  it("sells through a department key with the typed price", async () => {
    till(["department_keys"], { departments_list: () => departmentKeys() });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    // the module is read before its key is bound
    await screen.findByRole("button", { name: /Touches rayon/ });
    await user.keyboard("{Control>}5{/Control}");
    await user.click(await screen.findByRole("button", { name: "Légumes" }));
    await user.type(screen.getByLabelText("Prix"), "235");
    await user.click(screen.getByRole("button", { name: "Ajouter" }));
    await waitFor(() =>
      expect(useCart.getState().lines[0]?.line).toMatchObject({ productId: "op-veg", unitPrice: 23_500 }),
    );
  });

  it("asks one serial per phone and refuses an IMEI that fails its check", async () => {
    till(["serials"], {
      serials_available: () => serialsInStock(),
      products_find_by_code: () => phoneScanHit(),
      products_extras_get: () => phoneExtras(),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    // the scan field (M1b's combobox) shows once the open cash session is read
    await user.type(await screen.findByRole("combobox", { name: /Scanner/ }), "6130001112223{Enter}");
    const dialog = await screen.findByRole("dialog", { name: /Numéro de série/ });
    await user.type(within(dialog).getByLabelText("IMEI ou numéro de série"), "356938035643808{Enter}");
    expect(within(dialog).getByText("IMEI invalide")).toBeInTheDocument();
    await user.click(await within(dialog).findByRole("button", { name: "356938035643809" }));
    await user.click(within(dialog).getByRole("button", { name: "Valider" }));
    expect(useExtras.getState().serials).toEqual({ 0: ["356938035643809"] });
  });

  it("picks a size and a colour from the variant grid", async () => {
    till(["variant_grid"], { variant_grid: () => variantGrid() });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.click(await screen.findByRole("button", { name: /Chemise Oxford/ }));
    const grid = await screen.findByRole("grid", { name: "Chemise Oxford" });
    expect(within(grid).getByRole("gridcell", { name: "M Blanc 0" })).toBeDisabled();
    await user.click(within(grid).getByRole("gridcell", { name: "M Bleu 3" }));
    await waitFor(() => expect(useCart.getState().lines[0]?.line).toMatchObject({ productId: "v-m-bleu" }));
  });

  it("guesses the operator of a top-up from the number", async () => {
    till(["flexy"], {
      flexy_operators: () => topUpOperators(),
      flexy_line: (args) => ({
        productId: "svc-djezzy",
        description: null,
        qty: 1000,
        unitPrice: args.amount,
        discount: { kind: "none" },
        taxRateId: null,
        note: "0770 12 34 56",
        expectedUnitPrice: null,
      }),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await screen.findByRole("button", { name: /Flexy/ });
    await user.keyboard("{Control>}2{/Control}");
    await user.type(await screen.findByLabelText("Numéro"), "0770123456");
    expect(screen.getByRole("radio", { name: /Djezzy/ })).toBeChecked();
    await user.type(screen.getByLabelText("Montant"), "500");
    await user.click(screen.getByRole("button", { name: "Ajouter la recharge" }));
    await waitFor(() =>
      expect(useCart.getState().lines[0]?.line).toMatchObject({ productId: "svc-djezzy", unitPrice: 50_000 }),
    );
  });

  it("follows a remapped key", async () => {
    till(["department_keys"], {
      departments_list: () => departmentKeys(),
      till_key_map: () => keyMap({ department_keys: "Ctrl+9" }),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    // the key map is read before the keys move, and the button shows the new key
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /Touches rayon/ })).toHaveAttribute(
        "aria-keyshortcuts",
        "Control+9",
      ),
    );
    await user.keyboard("{Control>}9{/Control}");
    expect(await screen.findByRole("button", { name: "Légumes" })).toBeInTheDocument();
  });

  it("leaves the pack keys alone while their module is off", async () => {
    const calls = till([], { departments_list: () => departmentKeys() });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await screen.findByRole("button", { name: /Lait Candia/ });
    await user.keyboard("{Control>}5{/Control}");
    await user.keyboard("{Control>}2{/Control}");
    expect(screen.queryByRole("button", { name: "Légumes" })).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls).not.toContain("departments_list");
  });

  it("asks the prescription of a controlled medicine before the payment", async () => {
    till(["pharmacy"], {
      products_find_by_code: () => phoneScanHit(),
      pharmacy_product_get: ({ productId }) => ({
        productId,
        dci: "TRAMADOL",
        controlled: true,
        schedule: "II",
        prescriptionRequired: true,
      }),
      pharmacy_equivalents: () => [],
      chifa_of_product: () => null,
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.type(await screen.findByRole("combobox", { name: /Scanner/ }), "6130001112223{Enter}");
    await waitFor(() => expect(useCart.getState().lines).toHaveLength(1));
    await user.keyboard("{F12}");
    const form = await screen.findByRole("dialog", { name: "Registre des médicaments contrôlés" });
    await user.type(within(form).getByLabelText("Prescripteur"), "Dr Meziane");
    await user.type(within(form).getByLabelText("Date de l'ordonnance"), "2026-10-02");
    await user.type(within(form).getByLabelText("Patient"), "M. Haddad");
    await user.click(within(form).getByRole("button", { name: "Enregistrer et continuer" }));
    expect(await screen.findByRole("dialog", { name: "Encaissement" })).toBeInTheDocument();
    expect(useExtras.getState().prescriptions).toEqual([
      {
        line: 0,
        prescriberName: "Dr Meziane",
        prescriberRef: null,
        prescriptionNo: null,
        prescriptionDate: "2026-10-02",
        patientName: "M. Haddad",
        patientRef: null,
      },
    ]);
  });

  it("sells from the van of a round until the till goes back to the shop", async () => {
    till(["delivery_rounds"], {
      rounds_list: () => [
        {
          id: "r-4",
          number: "TR2026-000004",
          businessDate: "2026-10-03",
          driverName: "Rachid",
          driverUserId: null,
          vehiclePlate: null,
          vanLocationId: "loc-van-1",
          vanLocationName: "Camion 1",
          status: "out",
          note: null,
          startedAt: 1,
          closedAt: null,
          stops: [],
          totalDue: 0,
          totalCollected: 0,
          cashCollected: 0,
        },
      ],
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.keyboard("{Escape}");
    await user.click(await screen.findByRole("button", { name: "Vendre depuis le camion" }));
    await user.click(await screen.findByRole("button", { name: /Camion 1/ }));
    expect(await screen.findByText("Stock : Camion 1 — tournée TR2026-000004")).toBeInTheDocument();
    expect(useExtras.getState().toExtras().locationId).toBe("loc-van-1");
    await user.click(screen.getByRole("button", { name: "Revenir au magasin" }));
    expect(screen.queryByText(/tournée TR2026-000004/)).toBeNull();
    expect(useExtras.getState().toExtras().locationId).toBeUndefined();
  });

  it("asks the buyer of a jewel when the ticket has no identified customer", async () => {
    till(["jewellery"], {
      products_find_by_code: () => phoneScanHit(),
      jewellery_piece_get: ({ productId }) => ({ productId, metal: "gold" }),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.type(await screen.findByRole("combobox", { name: /Scanner/ }), "6130001112223{Enter}");
    await waitFor(() => expect(useCart.getState().lines).toHaveLength(1));
    await user.keyboard("{F12}");
    const form = await screen.findByRole("dialog", { name: "Acheteur (registre spécial)" });
    await user.click(within(form).getByRole("button", { name: "Continuer" }));
    expect(within(form).getAllByText("Renseignez ce champ.")).toHaveLength(3);
    await user.type(within(form).getByLabelText("Nom et prénom"), "Nadia Ouali");
    await user.type(within(form).getByLabelText("Numéro"), "109870123");
    await user.type(within(form).getByLabelText("Adresse"), "12 rue Larbi Ben M'hidi, Oran");
    await user.click(within(form).getByRole("button", { name: "Continuer" }));
    expect(await screen.findByRole("dialog", { name: "Encaissement" })).toBeInTheDocument();
    expect(useExtras.getState().buyer).toEqual({
      name: "Nadia Ouali",
      idKind: "cni",
      idNumber: "109870123",
      address: "12 rue Larbi Ben M'hidi, Oran",
    });
  });

  it("sets the ticket's customer from a scanned loyalty card", async () => {
    till(["loyalty"], { loyalty_card_lookup: () => customerFixture });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.type(await screen.findByRole("combobox", { name: /Scanner/ }), "2990000000017{Enter}");
    expect(await screen.findByText("Carte fidélité : Karim Benali")).toBeInTheDocument();
    expect(useCart.getState().customerId).toBe("c-karim");
  });

  it("offers a cheaper generic and shows the Chifa reference price under a medicine", async () => {
    till(["pharmacy"], {
      pharmacy_equivalents: () => [
        { productId: "p-gen", name: "Paracétamol Saidal 500", price: 18_000, onHand: 5_000, cheaper: true },
      ],
      chifa_of_product: () => ({
        code: "N02BE01",
        name: "Paracétamol 500",
        productId: "p-milk",
        referencePrice: 15_000,
        reimbursable: true,
      }),
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.click(await screen.findByRole("button", { name: /Lait Candia/ }));
    const cart = screen.getByRole("list", { name: "Ticket" });
    expect(
      await within(cart).findByText(/Générique moins cher : Paracétamol Saidal 500 180,00/),
    ).toBeInTheDocument();
    expect(within(cart).getByText(/Tarif de référence Chifa 150,00/)).toBeInTheDocument();
    expect(within(cart).getByRole("button", { name: "Remplacer" })).toBeInTheDocument();
  });

  it("shows a regulated staple's ceiling on its line", async () => {
    till(["regulated_prices"], {
      sales_preview: (args) => {
        const preview = cashierTill().sales_preview(args);
        preview.cart.lines = preview.cart.lines.map((l) => ({
          ...l,
          regulated: { code: "LAIT-SACHET", ceiling: 2_500, above: true },
        }));
        return preview;
      },
    });
    const user = userEvent.setup();
    await renderM8(<SaleScreen session={cashierTill().auth_session()} />);
    await user.click(await screen.findByRole("button", { name: /Lait Candia/ }));
    const cart = screen.getByRole("list", { name: "Ticket" });
    expect(await within(cart).findByText(/Prix plafonné 25,00/)).toBeInTheDocument();
    expect(within(cart).getByText("au-dessus du plafond")).toBeInTheDocument();
  });
});
