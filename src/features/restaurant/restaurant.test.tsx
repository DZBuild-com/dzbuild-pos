import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { previewFixture } from "@/test/fixtures-m1";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import {
  board,
  counter,
  modifierGroups,
  packState,
  productHit,
  sessionWith,
  ticket,
} from "@/test/fixtures-m8";
import { FloorScreen } from "./FloorScreen";
import { TicketScreen } from "./TicketScreen";

// src/test/setup.ts unmounts every screen, then clears the IPC mocks: the floor and the ticket
// listen to the LAN events until they unmount.

const cashier = sessionWith(["sales.create", "sales.hold"]);

describe("restaurant floor", () => {
  it("shows each table's state, time and total, and opens a free table with its covers", async () => {
    const opened: unknown[] = [];
    mockCommands({
      auth_session: () => cashier,
      packs_state: () => packState({ modules: ["restaurant"], tillMode: "restaurant" }),
      dining_rooms: () => [
        {
          id: "r1",
          code: "SALLE",
          nameAr: "القاعة",
          nameFr: "Salle",
          nameEn: "Dining room",
          gridCols: 4,
          gridRows: 3,
          sortOrder: 0,
          archived: false,
        },
      ],
      dining_board: () => board(),
      dining_counter: () => counter(),
      dining_ticket_open: (args) => {
        opened.push(args);
        return ticket({ id: "t9", tableCode: "T3", covers: 4 });
      },
    });
    const user = userEvent.setup();
    await renderM8(<FloorScreen session={cashier} />, { session: cashier });
    const t1 = await screen.findByRole("button", { name: /T1/ });
    expect(t1).toHaveAttribute("data-state", "occupied");
    expect(within(t1).getByText("12 min")).toBeInTheDocument();
    expect(within(t1).getByText("1 800,00 DA")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /T3/ }));
    await user.click(screen.getByRole("button", { name: "4" }));
    await user.click(screen.getByRole("button", { name: "Ouvrir la table" }));
    await waitFor(() =>
      expect(opened[0]).toMatchObject({ input: { mode: "dine_in", tableId: "tb3", covers: 4 } }),
    );
  });

  it("lays the floor out right to left in Arabic", async () => {
    mockCommands({
      auth_session: () => cashier,
      packs_state: () => packState({ modules: ["restaurant"], tillMode: "restaurant" }),
      dining_rooms: () => [],
      dining_board: () => board(),
      dining_counter: () => [],
    });
    await renderM8(<FloorScreen session={cashier} />, { lang: "ar", session: cashier });
    expect(await screen.findByRole("button", { name: "للأخذ" })).toBeInTheDocument();
    expect(document.documentElement.dir).toBe("rtl");
  });
});

describe("table ticket", () => {
  beforeEach(() => {
    // jsdom has no layout: M1b's product grid (TanStack Virtual) needs its scroller's size
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("asks the forced modifiers, sends to the kitchen and pays the ticket", async () => {
    const calls: Array<[string, unknown]> = [];
    const log = (name: string, value: unknown) => (args: unknown) => {
      calls.push([name, args]);
      return value;
    };
    mockCommands({
      auth_session: () => cashier,
      packs_state: () => packState({ modules: ["restaurant"], tillMode: "restaurant" }),
      dining_ticket_get: () => ticket({ id: "t1", tableCode: "T1" }),
      daily_menu_get: () => [],
      catalog_categories_list: () => [],
      products_grid: () => [
        productHit({ id: "p-burger", nameFr: "Burger", nameAr: "برغر", nameEn: "Burger", price: 60_000 }),
      ],
      modifiers_for_product: () => modifierGroups(),
      dining_ticket_add_line: log("add", ticket({ id: "t1", tableCode: "T1", lines: 1 })),
      dining_ticket_send: log("send", []),
      // the payment dialog's figures are the backend's, for the bill the payment takes
      dining_ticket_preview: () => previewFixture(180_000, 200_000),
      dining_ticket_pay: log("pay", {
        paid: { sale: { document: { id: "d1", number: "TK2026-000010" } } },
        printQueued: true,
      }),
    });
    const user = userEvent.setup();
    await renderM8(<TicketScreen session={cashier} ticketId="t1" />, { session: cashier });
    await user.click(await screen.findByRole("button", { name: /Burger/ }));
    const dialog = await screen.findByRole("dialog", { name: "Burger" });
    const add = within(dialog).getByRole("button", { name: "Ajouter" });
    expect(add).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: /À point/ }));
    await user.click(add);
    await waitFor(() => expect(calls[0]).toEqual(["add", expect.objectContaining({ ticketId: "t1" })]));
    await user.click(screen.getByRole("button", { name: "Envoyer en cuisine" }));
    await waitFor(() => expect(calls.map((c) => c[0])).toContain("send"));
    await user.keyboard("{F12}");
    await user.click(await screen.findByRole("button", { name: "Valider" }));
    await waitFor(() => expect(calls.map((c) => c[0])).toContain("pay"));
  });

  it("voids a sent line only with a reason, and asks a manager when the waiter may not", async () => {
    mockCommands({
      auth_session: () => sessionWith(["sales.create"]),
      packs_state: () => packState({ modules: ["restaurant"], tillMode: "restaurant" }),
      dining_ticket_get: () => ticket({ id: "t1", tableCode: "T1", lines: 1, sent: true }),
      daily_menu_get: () => [],
      auth_list_users: () => [],
    });
    const user = userEvent.setup();
    await renderM8(<TicketScreen session={sessionWith(["sales.create"])} ticketId="t1" />);
    await user.click(await screen.findByRole("button", { name: "Annuler la ligne Brochettes" }));
    const dialog = await screen.findByRole("dialog", { name: "Annuler une ligne envoyée" });
    expect(within(dialog).getByRole("button", { name: "Annuler la ligne" })).toBeDisabled();
    await user.click(within(dialog).getByRole("radio", { name: "Erreur de saisie" }));
    await user.click(within(dialog).getByRole("button", { name: "Annuler la ligne" }));
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
  });
});
