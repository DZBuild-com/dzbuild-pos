import { describe, expect, it } from "vitest";
import type {
  Closed,
  ReturnDone,
  Room,
  SaleDone,
  ScanHit,
  Session,
  StartupFailure,
  TableStatus,
  TemplateInfo,
} from "@/lib/ipc/types";
import { E2E_MARKER, createFakeBackend } from "@/test/e2e/fake-backend";

/** What a command refused with (the backend's `{ code, details }`), or `undefined`. */
function refusal(run: () => unknown): unknown {
  try {
    run();
  } catch (error) {
    return error;
  }
  return undefined;
}

const cart = (productId: string, qty: number) => ({
  lines: [
    {
      productId,
      description: null,
      qty,
      unitPrice: null,
      discount: { kind: "none" },
      taxRateId: null,
      note: null,
      expectedUnitPrice: null,
    },
  ],
  priceLevelId: null,
  customerId: null,
  lang: "fr",
  note: null,
});

const cash = (amount: number) => ({
  method: "cash",
  amount,
  reference: null,
  chequeBank: null,
  chequeDate: null,
});

describe("E2E fake backend", () => {
  it("runs a sale, a return and a Z like the real commands, and refuses unknown ones", () => {
    const call = createFakeBackend("shop");
    expect(call("app_startup_error", {})).toBeNull();
    expect(call("setup_status", {})).toMatchObject({ completed: true });
    expect(refusal(() => call("auth_login", { input: { user: "u-cashier", pin: "0000" } }))).toMatchObject({
      code: "invalid_pin",
    });
    const session = call("auth_login", { input: { user: "u-cashier", pin: "4826" } }) as Session;
    expect(session.displayName).toBe("Yacine");
    const hit = call("products_find_by_code", { code: "6130000000017" }) as ScanHit;
    expect(hit.product.sellPrice).toBe(12_000);
    const done = call("sales_create", {
      request: { sale: { cart: cart(hit.product.id, 2_000), payments: [cash(50_000)] }, printReceipt: false },
    }) as SaleDone;
    expect(done.sale.document.totalDue).toBe(24_000);
    expect(done.sale.settlement.change).toBe(26_000);
    expect(done.sale.document.number).toMatch(/^TK2026-C1-\d{6}$/);

    // A cashier returns goods only with a manager's PIN: refused, approved by Amine, then done.
    const line = done.sale.document.lines[0];
    const ret = {
      originalId: done.sale.document.id,
      lines: [
        {
          originLineId: line?.id ?? null,
          productId: null,
          qty: 1_000,
          unitPrice: null,
          restock: true,
          reason: "changed_mind",
          note: null,
        },
      ],
      refundMethod: "cash",
      refundReference: null,
      lang: "fr",
      note: null,
    };
    expect(refusal(() => call("returns_quote", { input: ret }))).toMatchObject({
      code: "insufficient_permission",
      details: { permission: "sales.return" },
    });
    call("auth_manager_override", {
      input: { approverId: "u-owner", pin: "2580", permission: "sales.return" },
    });
    expect(call("returns_quote", { input: ret })).toMatchObject({ value: 12_000 });
    const returned = call("returns_create", { request: { input: ret, printReceipt: false } }) as ReturnDone;
    expect(returned.returned.document.number).toBe("BR2026-C1-000001");
    expect(returned.returned.refund).toBe(12_000);

    const closed = call("cash_close_session", {
      input: { counts: [{ denomination: 100_000, count: 5 }], notes: null },
    }) as Closed;
    expect(closed.z.zNumber).toBe("ZC1-000001");
    expect(closed.z.countedCash).toBe(500_000);
    expect(call("cash_current", {})).toBeNull();

    expect(() => call("nothing_like_this", {})).toThrow(/unmocked command nothing_like_this/);
    expect(E2E_MARKER).toBe("__DZPOS_E2E_FAKE_BACKEND__");
    const fatal = createFakeBackend("fatal")("app_startup_error", {}) as StartupFailure;
    expect(fatal).toMatchObject({ error: { code: "database_corrupt" } });
  });

  it("starts empty for the first-run wizard, then signs the new owner in", () => {
    const call = createFakeBackend("fresh");
    expect(call("setup_status", {})).toMatchObject({ completed: false, hasOwner: false });
    const owner = call("setup_complete_first_run", {
      input: {
        lang: "fr",
        shopName: "Supérette El Amel",
        activity: "general",
        taxRegime: "ifu",
        priceMode: "ttc",
        wilaya: null,
        phone: null,
        ownerName: "Amine",
        ownerPin: "2580",
      },
    }) as Session;
    expect(owner).toMatchObject({ displayName: "Amine", isOwner: true });
    expect(call("auth_session", {})).toMatchObject({ displayName: "Amine" });
    expect(call("setup_status", {})).toMatchObject({ completed: true });
  });

  it("answers the wizard's templates and scale step and the pack state of the chosen activity (M8)", () => {
    const call = createFakeBackend("fresh");
    const templates = call("packs_templates", {}) as TemplateInfo[];
    expect(templates.map((t) => t.release)).toEqual(expect.arrayContaining(["mvp", "v1"]));
    expect(call("hardware_discover", {})).toEqual([]);
    expect(call("devices_status", {})).toEqual([]);
    call("setup_complete_first_run", {
      input: {
        lang: "fr",
        shopName: "Le Gourmet",
        activity: "food_service",
        taxRegime: "ifu",
        priceMode: "ttc",
        wilaya: null,
        phone: null,
        ownerName: "Amine",
        ownerPin: "2580",
      },
    });
    expect(call("packs_state", {})).toMatchObject({
      activity: "food_service",
      modules: ["restaurant", "staff"],
      tillMode: "restaurant",
      customised: false,
    });
  });

  it("opens a restaurant shop on its floor: one room, free tables, the counter and the editors (M8)", () => {
    const call = createFakeBackend("shop", { activity: "food_service" });
    expect(call("packs_state", {})).toMatchObject({
      tillMode: "restaurant",
      modules: ["restaurant", "staff"],
    });
    const rooms = call("dining_rooms", {}) as Room[];
    expect(rooms).toHaveLength(1);
    const board = call("dining_board", { roomId: rooms[0]?.id }) as TableStatus[];
    expect(board.map((s) => [s.table.code, s.state])).toEqual([
      ["T1", "free"],
      ["T2", "free"],
      ["T3", "free"],
      ["T4", "free"],
    ]);
    expect(call("dining_tables", { roomId: rooms[0]?.id })).toEqual(board.map((s) => s.table));
    expect(call("dining_counter", {})).toEqual([]);
    expect(call("settings_get", { key: "restaurant.predefined_tickets" })).toEqual(["Comptoir", "Terrasse"]);
    expect(call("kitchen_stations", {})).toEqual([]);
    expect(call("modifier_groups", {})).toEqual([]);
    expect(call("daily_menu_get", { day: "2026-09-28" })).toEqual([]);
    // the table ticket's screen, opened from its address (Task 32's screenshots)
    expect(call("dining_ticket_get", { id: "t1" })).toMatchObject({
      id: "t1",
      status: "open",
      tableId: board[0]?.table.id,
      tableCode: "T1",
      lines: [expect.objectContaining({ name: "Brochettes" })],
    });
  });

  it("keeps the shop in read-only mode once its trial ended (M9, Task 32's screenshots)", () => {
    const call = createFakeBackend("expired", { lang: "ar" });
    expect(call("setup_status", {})).toMatchObject({ completed: true });
    expect(call("license_status", {})).toMatchObject({ state: "expired", canWrite: false });
    expect(call("auth_login", { input: { user: "u-owner", pin: "2580" } })).toMatchObject({ lang: "ar" });
    expect(createFakeBackend("shop")("license_status", {})).toMatchObject({ canWrite: true });
  });
  it("opens a pharmacy and a butcher on their pack pages, empty (M8 Task 25)", () => {
    const pharmacy = createFakeBackend("shop", { activity: "pharmacy" });
    expect(pharmacy("packs_state", {})).toMatchObject({ modules: ["pharmacy", "lots", "staff"] });
    expect(pharmacy("pharmacy_expiry_board", { withinDays: 30 })).toEqual({
      expired: [],
      soon: [],
      expiredCost: 0,
      soonCost: 0,
    });
    expect(pharmacy("lots_expiry", { withinDays: null })).toEqual([]);
    expect(pharmacy("controlled_register_entries", { from: "2026-09-01", to: "2026-09-30" })).toEqual([]);
    expect(pharmacy("stock_locations", {})).not.toHaveLength(0);
    const butcher = createFakeBackend("shop", { activity: "butcher" });
    expect(butcher("packs_state", {})).toMatchObject({
      modules: expect.arrayContaining(["yield", "waste", "department_keys"]),
    });
    expect(butcher("yield_templates", {})).toEqual([]);
    expect(butcher("waste_day", { day: "2026-09-28" })).toEqual({ rows: [], totalCost: 0 });
    expect(butcher("departments_list", { includeArchived: false })).toEqual([]);
    expect(butcher("catalog_units_list", {})).not.toHaveLength(0);
    expect(butcher("catalog_tax_rates_list", {})).not.toHaveLength(0);
    expect(createFakeBackend("shop")("regulated_board", { levelId: null })).toEqual([]);
  });

  it("runs the general shop with the modules of every pack screen, no jewel nor regulated staple (M8 merge)", () => {
    const call = createFakeBackend("shop");
    expect(call("packs_state", {})).toMatchObject({
      activity: "general",
      modules: expect.arrayContaining(["loyalty", "repairs", "delivery_rounds", "jewellery", "zakat"]),
      customised: true,
    });
    // the till's checkout asks whether a line is a jewel; the regulated board reads the data pack
    expect(call("jewellery_piece_get", { productId: "p-semoule" })).toBeNull();
    expect(call("datapack_get", {})).toMatchObject({ regulated: { items: [] } });
    // Settings › Paiements: no custom payment type, no BaridPay QR yet (Task 32's screenshots)
    expect(call("payment_types_list", { includeArchived: true })).toEqual([]);
    expect(call("payments_baridpay_qr", {})).toBeNull();
  });
});
