import { readFileSync } from "node:fs";
import path from "node:path";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { TillEntry } from "@/features/pos/TillEntry";
import { FloorScreen } from "@/features/restaurant/FloorScreen";
import { TicketRoute } from "@/features/restaurant/TicketScreen";
import type { TableStatus, Ticket } from "@/lib/ipc/types";
import { previewFixture } from "@/test/fixtures-m1";
import {
  cashierTill,
  diningTable,
  keyMap,
  modifierGroups,
  packState,
  productHit,
  sessionWith,
  ticket,
  ticketLine,
  ticketSummary,
} from "@/test/fixtures-m8";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";

// RST-001…009 with the keyboard only (spec §10.5: "everything works with keyboard"), and the focus
// always visible (spec §10.1: a 3 px ring). The app's ring is the global `:focus-visible` outline
// of `src/styles/index.css`; an element that drops the outline draws its own ring instead.
const css = readFileSync(path.resolve(import.meta.dirname, "../styles/index.css"), "utf8");

function expectVisibleFocus(): HTMLElement {
  const focused = document.activeElement;
  expect(focused).toBeInstanceOf(HTMLElement);
  expect(focused).not.toBe(document.body);
  const classes = focused?.getAttribute("class") ?? "";
  if (/(^|\s)(focus(-visible)?:)?outline-none(\s|$)/.test(classes)) {
    expect(classes).toMatch(/(^|\s)focus(-visible)?:ring-\d/);
  }
  return focused as HTMLElement;
}

const waiter = sessionWith(["sales.create", "sales.hold"]);

/** The dining room: T1 occupied, T2 and T3 free on the first row, T4 free under T1. */
function floor(): TableStatus[] {
  return [
    { table: diningTable(), state: "occupied", tickets: [ticketSummary()] },
    { table: diningTable({ id: "tb2", code: "T2", gridCol: 1 }), state: "free", tickets: [] },
    { table: diningTable({ id: "tb3", code: "T3", gridCol: 2, seats: 6 }), state: "free", tickets: [] },
    { table: diningTable({ id: "tb4", code: "T4", gridRow: 1 }), state: "free", tickets: [] },
  ];
}

const room = {
  id: "r1",
  code: "SALLE",
  nameAr: "القاعة",
  nameFr: "Salle",
  nameEn: "Dining room",
  gridCols: 3,
  gridRows: 2,
  sortOrder: 0,
  archived: false,
};

const burger = productHit({
  id: "p-burger",
  nameFr: "Burger",
  nameAr: "برغر",
  nameEn: "Burger",
  price: 60_000,
});

/** A restaurant in the mixed mode (the sale screen with the floor one key away): the table ticket
 * the waiter opens, fills, sends and pays lives here. */
function restaurant() {
  let current: Ticket = ticket({ id: "t9", tableId: "tb3", tableCode: "T3", covers: 3, lines: [] });
  const writes: Array<[string, unknown]> = [];
  mockCommands({
    ...cashierTill(),
    auth_session: () => waiter,
    packs_state: () => packState({ modules: ["restaurant"], tillMode: "mixed" }),
    till_key_map: () => keyMap(),
    // the sale screen shows its (empty) ticket on the customer display
    display_show: () => null,
    dining_rooms: () => [room],
    dining_board: () => floor(),
    dining_counter: () => [],
    dining_ticket_open: (args) => {
      writes.push(["dining_ticket_open", args]);
      return current;
    },
    dining_ticket_get: () => current,
    daily_menu_get: () => [],
    products_search: () => [burger],
    modifiers_for_product: () => modifierGroups(),
    dining_ticket_add_line: (args) => {
      writes.push(["dining_ticket_add_line", args]);
      current = {
        ...current,
        lines: [ticketLine({ id: "l1", productId: "p-burger", name: "Burger", qty: 1_000 })],
      };
      return current;
    },
    dining_ticket_send: (args) => {
      writes.push(["dining_ticket_send", args]);
      current = { ...current, lines: current.lines.map((l) => ({ ...l, sentQty: l.qty })) };
      return [];
    },
    dining_ticket_preview: () => previewFixture(60_000, 60_000, 0),
    dining_ticket_pay: (args) => {
      writes.push(["dining_ticket_pay", args]);
      return { paid: { sale: { document: { id: "d1", number: "TK2026-000010" } } }, printQueued: true };
    },
  });
  return writes;
}

/** The till of `/pos/sale` (the floor's "Vente directe"), the floor and the table tickets. */
function till() {
  return renderM8(
    <Routes>
      <Route path="/pos/sale" element={<TillEntry session={waiter} direct />} />
      <Route path="/pos/floor" element={<FloorScreen session={waiter} />} />
      <Route path="/pos/ticket/:id" element={<TicketRoute session={waiter} />} />
    </Routes>,
    { route: "/pos/sale", session: waiter },
  );
}

beforeEach(() => {
  // jsdom has no layout: M1b's product grid (TanStack Virtual) needs its scroller's size
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
});

afterEach(() => {
  useCart.getState().clear("fr");
});

describe("the restaurant ticket with the keyboard only", () => {
  it("opens a table, adds a dish with its option, sends it to the kitchen and pays it", async () => {
    expect(css).toMatch(/:focus-visible\s*\{\s*outline:\s*3px solid var\(--focus\)/);
    const writes = restaurant();
    const user = userEvent.setup();
    await till();
    // the sale screen of the mixed mode: Ctrl+1 is the floor once the module is read
    await screen.findByRole("button", { name: /Tables/ });
    expect(expectVisibleFocus()).toHaveAttribute("id", "pos-scan");

    await user.keyboard("{Control>}1{/Control}");
    await screen.findByRole("heading", { name: "Salle" });
    // the floor takes the keyboard on its first table
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName(/^Table T1,/));
    expectVisibleFocus();
    await user.keyboard("{ArrowDown}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/^Table T4,/);
    await user.keyboard("{ArrowUp}{ArrowRight}{ArrowRight}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/^Table T3,/);
    // Tab leaves the floor's grid in one step (a roving focus): the counter comes next
    expect(screen.getByRole("button", { name: /^Table T2,/ })).toHaveAttribute("tabindex", "-1");

    await user.keyboard("{Enter}");
    await screen.findByRole("dialog", { name: "Table T3" });
    // the covers pad has the keyboard: the six seats proposed, the first digit typed replaces them
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName("Couverts"));
    expectVisibleFocus();
    await user.keyboard("3");
    expect(screen.getByRole("status", { name: "Couverts" })).toHaveTextContent("3");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(writes.map(([cmd]) => cmd)).toEqual(["dining_ticket_open"]));
    expect(writes[0]?.[1]).toMatchObject({ input: { mode: "dine_in", tableId: "tb3", covers: 3 } });

    // the table's ticket: the scan field has the keyboard once the ticket is read, F2 brings it
    // back from anywhere
    await screen.findByRole("heading", { level: 1, name: "Table T3" });
    await waitFor(() => expect(document.activeElement).toHaveAttribute("id", "pos-scan"));
    await user.keyboard("{F2}");
    expect(expectVisibleFocus()).toHaveAttribute("id", "pos-scan");
    await user.keyboard("burger");
    await screen.findByRole("option", { name: /Burger/ });
    await user.keyboard("{ArrowDown}{Enter}");

    // the forced option: the arrows move between the choices, Space picks one, Enter adds the line
    await screen.findByRole("dialog", { name: "Burger" });
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName(/Saignant/));
    expectVisibleFocus();
    await user.keyboard("{ArrowRight}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/À point/);
    await user.keyboard(" ");
    expect(screen.getByRole("button", { name: /À point/ })).toHaveAttribute("aria-pressed", "true");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(writes[1]).toEqual([
      "dining_ticket_add_line",
      expect.objectContaining({
        ticketId: "t9",
        input: expect.objectContaining({ modifiers: ["mo-medium"] }),
      }),
    ]);
    await waitFor(() => expect(document.activeElement).toHaveAttribute("id", "pos-scan"));
    expectVisibleFocus();

    // "Envoyer en cuisine" has its key (F6, the sale screen's "Attente": the ticket waits)
    const send = await screen.findByRole("button", { name: "Envoyer en cuisine" });
    expect(send).toHaveAttribute("aria-keyshortcuts", "F6");
    // the ticket read again holds the line to send
    await waitFor(() => expect(send).toBeEnabled());
    await user.keyboard("{F6}");
    expect(await screen.findByText("Envoyé en cuisine.")).toBeInTheDocument();
    expectVisibleFocus();

    // F12 pays: the payment dialog's amount has the keyboard, Enter validates
    await user.keyboard("{F12}");
    await screen.findByRole("dialog", { name: "Encaissement" });
    await waitFor(() => expect(document.activeElement?.tagName).toBe("INPUT"));
    expectVisibleFocus();
    await waitFor(() => expect(screen.getByRole("button", { name: "Valider" })).toBeEnabled());
    await user.keyboard("{Enter}");

    // back on the floor, its first table has the keyboard again
    await screen.findByRole("heading", { name: "Salle" });
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName(/^Table T1,/));
    expectVisibleFocus();
    expect(writes.map(([cmd]) => cmd)).toEqual([
      "dining_ticket_open",
      "dining_ticket_add_line",
      "dining_ticket_send",
      "dining_ticket_pay",
    ]);
  });

  it("asks before leaving a table with dishes not sent to the kitchen (final review)", async () => {
    const writes = restaurant();
    const user = userEvent.setup();
    await renderM8(
      <Routes>
        <Route path="/pos/floor" element={<FloorScreen session={waiter} />} />
        <Route path="/pos/ticket/:id" element={<TicketRoute session={waiter} />} />
      </Routes>,
      { route: "/pos/ticket/t9", session: waiter },
    );
    await screen.findByRole("heading", { level: 1, name: "Table T3" });
    await waitFor(() => expect(document.activeElement).toHaveAttribute("id", "pos-scan"));
    await user.keyboard("burger");
    await screen.findByRole("option", { name: /Burger/ });
    await user.keyboard("{ArrowDown}{Enter}");
    await screen.findByRole("dialog", { name: "Burger" });
    // the forced option: Space picks the first choice, Enter adds the line
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName(/Saignant/));
    await user.keyboard(" {Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(screen.getByRole("button", { name: "Envoyer en cuisine" })).toBeEnabled());
    // Esc with a line still to send: the question, "Envoyer en cuisine" on Enter
    await user.keyboard("{Escape}");
    const ask = await screen.findByRole("dialog", { name: "Des plats ne sont pas envoyés en cuisine" });
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName("Envoyer en cuisine"));
    expectVisibleFocus();
    expect(ask).toHaveTextContent("Quitter sans envoyer");
    await user.keyboard("{Enter}");
    await screen.findByRole("heading", { name: "Salle" });
    expect(writes.map(([cmd]) => cmd)).toEqual(["dining_ticket_add_line", "dining_ticket_send"]);
  });

  it("marks the tables and counter tickets with dishes still to send", async () => {
    restaurant();
    mockCommands({
      dining_board: () => [
        {
          table: diningTable(),
          state: "occupied",
          tickets: [ticketSummary({ unsentLines: 2 })],
        },
      ],
      dining_counter: () => [
        ticketSummary({
          id: "t5",
          tableId: null,
          tableCode: null,
          mode: "takeaway",
          orderNo: 12,
          unsentLines: 1,
        }),
      ],
    });
    await renderM8(<FloorScreen session={waiter} />, { route: "/pos/floor", session: waiter });
    const tile = await screen.findByRole("button", { name: /^Table T1,/ });
    expect(tile).toHaveTextContent("À envoyer");
    expect(await screen.findByRole("button", { name: /N° 12/ })).toHaveTextContent("À envoyer");
  });

  it("follows the screen's direction in Arabic: the left arrow goes to the next table", async () => {
    restaurant();
    const user = userEvent.setup();
    await renderM8(<FloorScreen session={waiter} />, { lang: "ar", route: "/pos/floor", session: waiter });
    await waitFor(() => expect(document.activeElement).toHaveAccessibleName(/T1/));
    await user.keyboard("{ArrowLeft}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/T2/);
    await user.keyboard("{ArrowRight}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/T1/);
    await user.keyboard("{End}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/T4/);
    await user.keyboard("{Home}");
    expect(expectVisibleFocus()).toHaveAccessibleName(/T1/);
  });
});
