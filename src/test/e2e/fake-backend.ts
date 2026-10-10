/**
 * The E2E suite's backend (M6 Task 19): an in-memory shop answering the commands the critical
 * flows call, with the generated binding types, so the Playwright specs drive the real UI without
 * Rust. Money totals are computed simply (price × quantity, no tax, no stamp duty): the suite tests
 * screens and keys, the tax rules are pinned by the Rust tests. A command this file does not know
 * throws `unmocked command <name>`, so a screen that starts calling a new one fails the suite.
 *
 * Never in a production build: `src/main.tsx` imports it only when `VITE_E2E=1`, and
 * `pnpm perf:bundle` refuses a chunk that carries {@link E2E_MARKER}.
 */
import type { Lang } from "@/lib/i18n";
import { DEFAULT_KEYS } from "@/lib/key-map";
import type {
  Appointment,
  CartLine,
  CashMovement,
  CashSession,
  Category,
  Closed,
  CloseInput,
  CloudNotice,
  Company,
  Consent,
  DemoStatus,
  DeviceStatus,
  DiningTable,
  Discovered,
  DisplayPrefs,
  Document,
  DocumentLine,
  DocumentSummary,
  ExpiryBoard,
  FailedItem,
  FirstRunInput,
  FloatEntry,
  HardwareConfig,
  HeldCart,
  InstalledPack,
  LicenseStatus,
  LoginInput,
  MetalPrice,
  MirrorStatus,
  Module,
  OnlineOrderSummary,
  OrderSummary,
  OverrideGrant,
  OverrideInput,
  Pack,
  PackState,
  PaymentInput,
  PaymentPreview,
  Permission,
  Plan,
  PreviewInput,
  PricedCart,
  Product,
  ProductHit,
  RecoveryBackup,
  Reminder,
  RemovableDrive,
  ReportInfo,
  ReportRequest,
  ReportView,
  ReturnableLine,
  ReturnDone,
  ReturnInput,
  ReturnQuote,
  ReturnRequest,
  Round,
  Room,
  SaleDone,
  SaleRequest,
  ScanHit,
  SearchQuery,
  ServiceSummary,
  Session,
  SessionSummary,
  Settlement,
  SetupStatus,
  ShiftRow,
  StaffProfile,
  ShopActivity,
  StartupFailure,
  StartupNotice,
  SupportContact,
  TableStatus,
  TemplateInfo,
  UserTile,
  Voucher,
  ZakatPreview,
  ZakatStatement,
  WasteDay,
  ZReport,
} from "@/lib/ipc/types";
import {
  appInfoFixture,
  backupFixture,
  backupStatusFixture,
  cashierSession,
  companyFixture,
  ownerSession,
  packFixture,
  settingsValues,
  updateStatusFixture,
} from "@/test/fixtures";
import {
  cashSessionFixture,
  documentFixture,
  priceLevelsFixture,
  productFixture,
  taxRatesFixture,
  unitsFixture,
  zReportFixture,
} from "@/test/fixtures-m1";
import { dashboardFixture, locationsFixture } from "@/test/fixtures-m2";
import { lanStandaloneFixture } from "@/test/fixtures-m5";
import { unlinkedStatus } from "@/test/fixtures-m7";
import { flexyOperators, ticket } from "@/test/fixtures-m8";
import { activeStatus, expiredStatus, unlicensedStatus } from "@/test/fixtures-m9";

/** Task 17's `pnpm perf:bundle` refuses a production chunk that contains this string. */
export const E2E_MARKER = "__DZPOS_E2E_FAKE_BACKEND__";

/** `fresh`: the first start (setup wizard); `shop`: a shop with its users, products and an open
 * till; `fatal`: a database that cannot open (the fatal screen); `unlicensed`: a new install
 * without a licence (the activation screen); `expired`: the shop whose trial ended, in read-only
 * mode (the till's card, the back office's banner). */
export type FakeMode = "fresh" | "shop" | "fatal" | "unlicensed" | "expired";

export type FakeBackend = (cmd: string, args: Record<string, unknown>) => unknown;

export interface FakeOptions {
  /** The language of the shop's users and of the login screen (screenshots in each language). */
  lang?: Lang;
  /** The shop's activity (M8: `food_service` opens the till on the restaurant's floor; `pharmacy`
   * and `butcher` turn their modules on). */
  activity?: ShopActivity;
}

/** The fixed day of the fake shop: the numbers carry its year. */
const BUSINESS_DATE = cashSessionFixture.businessDate;
const YEAR = 2026;
const TERMINAL = "C1";

interface FakeUser {
  tile: UserTile;
  session: Session;
  pin: string;
}

interface FakeState {
  setupDone: boolean;
  company: Company;
  users: FakeUser[];
  session: Session | null;
  /** Permissions a manager lifted for the next operation (`auth_manager_override`). */
  grants: Permission[];
  settings: Record<string, unknown>;
  /** This terminal's open cash session. */
  till: CashSession | null;
  closedSessions: CashSession[];
  /** By cash session id. */
  zReports: Map<string, ZReport>;
  documents: Document[];
  counters: { ticket: number; returnNote: number; z: number };
  license: LicenseStatus;
}

/** A fresh install: no printer, a scanner in US keyboard emulation. */
const HARDWARE: HardwareConfig = {
  version: 1,
  printers: [],
  receiptPrinterId: null,
  scanner: { layout: "us" },
};

/** Five of the 25 activity templates (M8): enough for the wizard's two groups, the settings and
 * the restaurant, pharmacy and butcher screens. */
const TEMPLATES: readonly TemplateInfo[] = [
  {
    activity: "general",
    release: "mvp",
    modules: ["department_keys", "regulated_prices", "loyalty"],
    tillMode: "retail",
    categories: [{ ar: "مواد غذائية", fr: "Épicerie", en: "Groceries" }],
    services: [],
  },
  {
    activity: "food_service",
    release: "mvp",
    modules: ["restaurant", "staff"],
    tillMode: "restaurant",
    categories: [{ ar: "أطباق", fr: "Plats", en: "Dishes" }],
    services: [],
  },
  {
    activity: "pharmacy",
    release: "mvp",
    modules: ["pharmacy", "lots", "staff"],
    tillMode: "retail",
    categories: [{ ar: "أدوية", fr: "Médicaments", en: "Medicines" }],
    services: [],
  },
  {
    activity: "butcher",
    release: "mvp",
    modules: ["department_keys", "yield", "waste", "loyalty", "staff", "zakat"],
    tillMode: "retail",
    categories: [{ ar: "لحم أحمر", fr: "Viande rouge", en: "Red meat" }],
    services: [{ ar: "تقطيع وتحضير", fr: "Découpe et préparation", en: "Cutting and preparation" }],
  },
  {
    activity: "jewellery",
    release: "v1",
    modules: ["jewellery", "staff", "zakat"],
    tillMode: "retail",
    categories: [{ ar: "خواتم", fr: "Bagues", en: "Rings" }],
    services: [],
  },
];

/** The restaurant's room (M8): "Salle", 4 × 3 cells, four free tables on its first row. */
const ROOM: Room = {
  id: "00000000-0000-7000-8000-000000000801",
  code: "SALLE",
  nameAr: "القاعة",
  nameFr: "Salle",
  nameEn: "Dining room",
  gridCols: 4,
  gridRows: 3,
  sortOrder: 1,
  archived: false,
};
const TABLES: DiningTable[] = ["T1", "T2", "T3", "T4"].map((code, i) => ({
  id: `00000000-0000-7000-8000-00000000081${i}`,
  roomId: ROOM.id,
  code,
  seats: 4,
  shape: "square",
  gridCol: i,
  gridRow: 0,
  archived: false,
}));

/** The shop's pack state: its activity's template, nothing chosen by hand. */
/** The modules the general fake shop runs beyond its template, so the pack screens
 * (repairs, flexy, instalments, orders and appointments, wholesale and rounds, vouchers,
 * jewellery, staff, zakat) open behind their `PackGate` in the E2E screenshots. */
const GENERAL_EXTRA: readonly Module[] = [
  "repairs",
  "flexy",
  "instalments",
  "orders",
  "appointments",
  "wholesale",
  "delivery_rounds",
  "gift_vouchers",
  "jewellery",
  "staff",
  "zakat",
];

function packStateOf(activity: ShopActivity): PackState {
  const template = TEMPLATES.find((t) => t.activity === activity) ?? TEMPLATES[0];
  const templateModules = template ? [...template.modules] : [];
  const extra = activity === "general" ? GENERAL_EXTRA.filter((m) => !templateModules.includes(m)) : [];
  return {
    activity,
    modules: [...templateModules, ...extra],
    tillMode: template?.tillMode ?? "retail",
    templateModules,
    customised: extra.length > 0,
  };
}
/** This PC's display: full mode, normal text, no on-screen keyboard. */
const DISPLAY: DisplayPrefs = { lightMode: false, textSize: "normal", onscreenKeyboard: false };

type Names = [nameAr: string, nameFr: string, nameEn: string];

function category(id: string, [nameAr, nameFr, nameEn]: Names, sortOrder: number): Category {
  return { id, parentId: null, nameAr, nameFr, nameEn, color: null, sortOrder, image: null, archived: false };
}

const CATEGORIES: Category[] = [
  category("c-grocery", ["البقالة", "Épicerie", "Grocery"], 1),
  category("c-drinks", ["المشروبات", "Boissons", "Drinks"], 2),
  category("c-dairy", ["الألبان", "Crèmerie", "Dairy"], 3),
  category("c-hygiene", ["النظافة", "Hygiène", "Hygiene"], 4),
];

/** A piece product at `sellPrice` centimes, sold by its EAN-13 `barcode`. */
function product(
  id: string,
  categoryId: string,
  [nameAr, nameFr, nameEn]: Names,
  sellPrice: number,
  barcode: string,
): Product {
  return {
    ...productFixture,
    id,
    reference: id.slice(2).toUpperCase(),
    nameAr,
    nameFr,
    nameEn,
    categoryId,
    taxRateBp: 0,
    sellPrice,
    minStock: 0,
    barcodes: [{ barcode, packQty: 1_000, label: null }],
  };
}

/** Twelve products of a supérette. */
const PRODUCTS: Product[] = [
  product(
    "p-semoule",
    "c-grocery",
    ["سميد متوسط 1 كغ", "Semoule moyenne 1 kg", "Medium semolina 1 kg"],
    12_000,
    "6130000000017",
  ),
  product(
    "p-eau",
    "c-drinks",
    ["مياه معدنية 1.5 لتر", "Eau minérale 1,5 L", "Mineral water 1.5 L"],
    4_500,
    "6130000000024",
  ),
  product("p-lait", "c-dairy", ["حليب 1 لتر", "Lait UHT 1 L", "UHT milk 1 L"], 15_000, "6130000000031"),
  product("p-huile", "c-grocery", ["زيت 5 لتر", "Huile 5 L", "Oil 5 L"], 65_000, "6130000000048"),
  product("p-sucre", "c-grocery", ["سكر 1 كغ", "Sucre 1 kg", "Sugar 1 kg"], 11_000, "6130000000055"),
  product(
    "p-cafe",
    "c-grocery",
    ["قهوة 250 غ", "Café moulu 250 g", "Ground coffee 250 g"],
    32_000,
    "6130000000062",
  ),
  product("p-pates", "c-grocery", ["عجائن 500 غ", "Pâtes 500 g", "Pasta 500 g"], 9_000, "6130000000079"),
  product(
    "p-the",
    "c-grocery",
    ["شاي أخضر 500 غ", "Thé vert 500 g", "Green tea 500 g"],
    45_000,
    "6130000000086",
  ),
  product(
    "p-jus",
    "c-drinks",
    ["عصير برتقال 1 لتر", "Jus d'orange 1 L", "Orange juice 1 L"],
    16_000,
    "6130000000093",
  ),
  product("p-yaourt", "c-dairy", ["ياغورت", "Yaourt nature", "Plain yoghurt"], 3_000, "6130000000109"),
  product("p-savon", "c-hygiene", ["صابون 125 غ", "Savon 125 g", "Soap 125 g"], 7_500, "6130000000116"),
  product(
    "p-javel",
    "c-hygiene",
    ["ماء جافيل 1 لتر", "Eau de Javel 1 L", "Bleach 1 L"],
    8_500,
    "6130000000123",
  ),
];

function hitOf(product: Product): ProductHit {
  return {
    id: product.id,
    reference: product.reference,
    nameAr: product.nameAr,
    nameFr: product.nameFr,
    nameEn: product.nameEn,
    kind: product.kind,
    categoryId: product.categoryId,
    unitCode: product.unitCode,
    sellPrice: product.sellPrice,
    barcode: product.barcodes[0]?.barcode ?? null,
    stock: 50_000,
    image: null,
    archived: false,
    availableAtTill: true,
  };
}

function nameIn(product: Product, lang: Lang): string {
  return lang === "ar" ? product.nameAr : lang === "en" ? product.nameEn : product.nameFr;
}

/** A refusal of the backend, as `toAppError` reads it. */
function refuse(code: string, details?: Record<string, unknown>): never {
  throw details === undefined ? { code } : { code, details };
}

const HASH = "0".repeat(64);
const NOW = 1_790_593_200_000;

/** The in-memory shop; each call answers like the Tauri command of the same name. */
export function createFakeBackend(mode: FakeMode, { lang = "fr", activity }: FakeOptions = {}): FakeBackend {
  const owner: FakeUser = {
    tile: {
      id: ownerSession.userId,
      displayName: ownerSession.displayName,
      roleId: ownerSession.roleId,
      roleNameAr: ownerSession.roleNameAr,
      roleNameFr: ownerSession.roleNameFr,
      roleNameEn: ownerSession.roleNameEn,
      lang,
    },
    session: { ...ownerSession, lang },
    pin: "2580",
  };
  const cashier: FakeUser = {
    tile: {
      id: cashierSession.userId,
      displayName: cashierSession.displayName,
      roleId: cashierSession.roleId,
      roleNameAr: cashierSession.roleNameAr,
      roleNameFr: cashierSession.roleNameFr,
      roleNameEn: cashierSession.roleNameEn,
      lang,
    },
    session: { ...cashierSession, lang },
    pin: "4826",
  };
  const fresh = mode === "fresh" || mode === "unlicensed";
  const state: FakeState = {
    setupDone: !fresh,
    company: { ...companyFixture },
    users: fresh ? [] : [owner, cashier],
    session: null,
    grants: [],
    settings: {
      ...settingsValues,
      "ui.login_lang": lang,
      "restaurant.predefined_tickets": ["Comptoir", "Terrasse"],
      ...(activity === undefined ? {} : { "shop.activity": activity }),
    },
    till: fresh ? null : { ...cashSessionFixture },
    closedSessions: [],
    zReports: new Map(),
    documents: [],
    counters: { ticket: 0, returnNote: 0, z: 0 },
    license: mode === "unlicensed" ? unlicensedStatus : mode === "expired" ? expiredStatus : activeStatus,
  };

  const userOf = (id: string): FakeUser | undefined => state.users.find((u) => u.tile.id === id);

  const signedIn = (): Session => state.session ?? refuse("session_required");

  /** What the real `require` does, with the manager's grant for the next operation. */
  const require = (permission: Permission): Session => {
    const session = signedIn();
    if (!session.permissions.includes(permission) && !state.grants.includes(permission)) {
      refuse("insufficient_permission", { permission });
    }
    return session;
  };

  const productOf = (id: string | null): Product | undefined => PRODUCTS.find((p) => p.id === id);

  /** The cart priced simply: the catalogue price (or the typed one) × the quantity. */
  const price = (input: PreviewInput): PricedCart => {
    const lines = input.cart.lines.map((line: CartLine, i) => {
      const product = productOf(line.productId);
      const unitPrice = line.unitPrice ?? product?.sellPrice ?? 0;
      const total = Math.round((unitPrice * line.qty) / 1_000);
      return {
        lineNo: i + 1,
        productId: line.productId,
        kind: product?.kind ?? null,
        description: product ? nameIn(product, input.cart.lang) : (line.description ?? ""),
        qty: line.qty,
        unitCode: product?.unitCode ?? "pc",
        unitPrice,
        // no tax in the fake shop: the TTC unit price is the price
        unitPriceTtc: unitPrice,
        cataloguePrice: product?.sellPrice ?? null,
        priceOverridden: line.unitPrice !== null,
        priceChanged: false,
        taxRateBp: 0,
        amounts: { gross: total, discountBp: 0, discountAmount: 0, totalHt: total, tax: 0, totalTtc: total },
        costPrice: 0,
        note: line.note,
      };
    });
    const totalTtc = lines.reduce((sum, l) => sum + l.amounts.totalTtc, 0);
    return {
      lines,
      totals: {
        grossTotal: totalTtc,
        subtotalHt: totalTtc,
        discountTotal: 0,
        taxTotal: 0,
        totalTtc,
        perRate: [],
      },
      priceLevelId: "00000000-0000-7000-8000-000000000201",
      priceMode: "ttc",
      taxRegime: "ifu",
      lang: input.cart.lang,
      itemCount: lines.reduce((sum, l) => sum + l.qty, 0),
    };
  };

  const settle = (totalTtc: number, tenders: PaymentInput[]): Settlement => {
    const nonCash = tenders.filter((t) => t.method !== "cash").reduce((s, t) => s + t.amount, 0);
    const cashTendered = tenders.filter((t) => t.method === "cash").reduce((s, t) => s + t.amount, 0);
    const cashDue = totalTtc - nonCash;
    return {
      totalTtc,
      stampBase: 0,
      stampDuty: 0,
      stampMode: "off",
      stampCharged: 0,
      rounding: 0,
      totalDue: totalTtc,
      storeCredit: 0,
      nonCash,
      cashDue,
      cashTendered,
      paidTotal: nonCash + cashTendered,
      missing: Math.max(0, cashDue - cashTendered),
      change: Math.max(0, cashTendered - cashDue),
      tenders,
    };
  };

  const preview = (input: PreviewInput): PaymentPreview => {
    const cart = price(input);
    return { cart, settlement: settle(cart.totals.totalTtc, input.tenders) };
  };

  const openTill = (): CashSession => state.till ?? refuse("cash_session_closed");

  const document = (
    docType: "ticket" | "return_note",
    number: string,
    session: Session,
    lines: DocumentLine[],
    payments: Document["payments"],
    extra: Partial<Document>,
  ): Document => {
    const totalTtc = lines.reduce((sum, l) => sum + l.totalTtc, 0);
    return {
      ...documentFixture,
      id: `d-${state.documents.length + 1}`,
      docType,
      number,
      series: TERMINAL,
      businessDate: BUSINESS_DATE,
      userId: session.userId,
      cashSessionId: openTill().id,
      lang: extra.lang ?? "fr",
      subtotalHt: totalTtc,
      taxTotal: 0,
      totalTtc,
      stampDuty: 0,
      stampMode: "off",
      totalDue: totalTtc,
      paidTotal: totalTtc,
      changeGiven: 0,
      validatedAt: NOW + state.documents.length * 60_000,
      chainSeq: state.documents.length + 1,
      prevHash: HASH,
      hash: HASH,
      createdAt: NOW,
      lines,
      taxes: [],
      payments,
      ...extra,
    };
  };

  const sale = ({ sale: { cart, payments } }: SaleRequest): SaleDone => {
    const session = require("sales.create");
    const till = openTill();
    const priced = price({ cart, tenders: payments, storeCredit: null });
    const settlement = settle(priced.totals.totalTtc, payments);
    if (settlement.missing > 0) refuse("payment_insufficient");
    state.counters.ticket += 1;
    const number = `TK${YEAR}-${TERMINAL}-${String(state.counters.ticket).padStart(6, "0")}`;
    const lines = priced.lines.map((l, i): DocumentLine => ({
      id: `l-${state.documents.length + 1}-${i + 1}`,
      lineNo: l.lineNo,
      productId: l.productId,
      description: l.description,
      descriptionAlt: null,
      qty: l.qty,
      unitCode: l.unitCode,
      unitPrice: l.unitPrice,
      priceOverridden: l.priceOverridden,
      discountBp: 0,
      discountAmount: 0,
      taxRateBp: 0,
      totalHt: l.amounts.totalTtc,
      taxAmount: 0,
      totalTtc: l.amounts.totalTtc,
      costPrice: 0,
      lotId: null,
      serialNo: null,
      parentLineId: null,
      originLineId: null,
      restockLocationId: null,
      reasonCode: null,
      note: l.note,
    }));
    const paid = payments.map((p, i) => ({ ...p, id: `pay-${number}-${i}`, cashSessionId: till.id }));
    const change =
      settlement.change > 0
        ? [
            {
              id: `pay-${number}-change`,
              method: "change" as const,
              amount: -settlement.change,
              reference: null,
              chequeBank: null,
              chequeDate: null,
              cashSessionId: till.id,
            },
          ]
        : [];
    const doc = document("ticket", number, session, lines, [...paid, ...change], {
      lang: cart.lang,
      paidTotal: settlement.paidTotal,
      changeGiven: settlement.change,
    });
    state.documents.push(doc);
    return { sale: { document: doc, settlement, shortages: [] }, printQueued: false };
  };

  const returned = (originLineId: string | null): number =>
    state.documents
      .filter((d) => d.docType === "return_note")
      .flatMap((d) => d.lines)
      .filter((l) => l.originLineId === originLineId)
      .reduce((sum, l) => sum - l.qty, 0);

  const returnable = (originalId: string): ReturnableLine[] => {
    const original = state.documents.find((d) => d.id === originalId) ?? refuse("document_not_found");
    return original.lines.map((line) => {
      const back = returned(line.id);
      return { line, returned: back, remaining: line.qty - back };
    });
  };

  /** The lines of a return with their value, refused like the real one. */
  const quoteLines = (input: ReturnInput) => {
    require("sales.return");
    openTill();
    if (input.originalId === null) refuse("return_without_receipt_not_allowed");
    const lines = returnable(input.originalId);
    return input.lines.map((asked) => {
      const sold = lines.find((l) => l.line.id === asked.originLineId) ?? refuse("record_not_found");
      if (asked.qty <= 0 || asked.qty > sold.remaining) refuse("return_qty_exceeds_sold");
      return { sold: sold.line, asked, amount: Math.round((sold.line.unitPrice * asked.qty) / 1_000) };
    });
  };

  const quote = (input: ReturnInput): ReturnQuote => {
    const lines = quoteLines(input);
    return {
      value: lines.reduce((sum, l) => sum + l.amount, 0),
      lines: lines.map((l) => ({
        originLineId: l.sold.id,
        productId: l.sold.productId,
        qty: l.asked.qty,
        amount: l.amount,
      })),
    };
  };

  const createReturn = ({ input }: ReturnRequest): ReturnDone => {
    const session = signedIn();
    const lines = quoteLines(input);
    const refund = lines.reduce((sum, l) => sum + l.amount, 0);
    state.counters.returnNote += 1;
    const number = `BR${YEAR}-${TERMINAL}-${String(state.counters.returnNote).padStart(6, "0")}`;
    const docLines: DocumentLine[] = lines.map((l, i) => ({
      ...l.sold,
      id: `l-${number}-${i + 1}`,
      lineNo: i + 1,
      qty: -l.asked.qty,
      totalHt: -l.amount,
      totalTtc: -l.amount,
      originLineId: l.sold.id,
      reasonCode: l.asked.reason,
    }));
    const doc = document(
      "return_note",
      number,
      session,
      docLines,
      [
        {
          id: `pay-${number}`,
          method: input.refundMethod,
          amount: -refund,
          reference: input.refundReference,
          chequeBank: null,
          chequeDate: null,
          cashSessionId: openTill().id,
        },
      ],
      { lang: input.lang, parentId: input.originalId, totalDue: -refund, paidTotal: -refund },
    );
    state.documents.push(doc);
    // the manager's approval covered this return only
    state.grants = [];
    return { returned: { document: doc, refund, storeCredit: 0 }, printQueued: false };
  };

  const summary = (till: CashSession): SessionSummary => {
    const docs = state.documents.filter((d) => d.cashSessionId === till.id);
    const tickets = docs.filter((d) => d.docType === "ticket");
    const returns = docs.filter((d) => d.docType === "return_note");
    const salesTtc = tickets.reduce((sum, d) => sum + d.totalTtc, 0);
    const returnsTtc = returns.reduce((sum, d) => sum + d.totalTtc, 0);
    const cashOf = (list: Document[]) =>
      list
        .flatMap((d) => d.payments)
        .reduce((s, p) => s + (p.method === "cash" || p.method === "change" ? p.amount : 0), 0);
    const cashSales = cashOf(tickets);
    const cashRefunds = cashOf(returns);
    return {
      session: till,
      terminalCode: TERMINAL,
      cashierName: userOf(till.userId)?.tile.displayName ?? "",
      tickets: tickets.length,
      returns: returns.length,
      firstTicket: tickets[0]?.number ?? null,
      lastTicket: tickets.at(-1)?.number ?? null,
      salesTtc,
      returnsTtc,
      netTtc: salesTtc + returnsTtc,
      discountTotal: 0,
      perRate: [],
      stampCharged: 0,
      stampAbsorbed: 0,
      rounding: 0,
      byMethod: [
        {
          method: "cash",
          received: cashSales,
          paidBack: cashRefunds,
          net: cashSales + cashRefunds,
          count: tickets.length + returns.length,
        },
      ],
      openingFloat: till.openingFloat,
      cashSales,
      cashRefunds,
      paidIn: 0,
      paidOut: 0,
      drops: 0,
      expenses: [],
      expensesTotal: 0,
      expectedCash: till.openingFloat + cashSales + cashRefunds,
    };
  };

  const close = ({ counts, notes }: CloseInput): Closed => {
    const session = require("cash.close");
    const till = openTill();
    const before = summary(till);
    const countedCash = counts.reduce((sum, c) => sum + c.denomination * c.count, 0);
    state.counters.z += 1;
    const zNumber = `Z${TERMINAL}-${String(state.counters.z).padStart(6, "0")}`;
    const closedSession: CashSession = {
      ...till,
      status: "closed",
      closedAt: NOW + 3_600_000,
      closedBy: session.userId,
      expectedCash: before.expectedCash,
      countedCash,
      variance: countedCash - (before.expectedCash ?? 0),
      zNumber,
    };
    const z: ZReport = {
      ...zReportFixture,
      zNumber,
      zSeq: state.counters.z,
      summary: { ...before, session: closedSession },
      counts,
      countedCash,
      variance: closedSession.variance ?? 0,
      grandTotal: before.netTtc,
      documentAnchors: [
        { docType: "ticket", series: TERMINAL, lastSeq: state.counters.ticket, lastHash: HASH, intact: true },
      ],
      closedAt: closedSession.closedAt ?? NOW,
      closedBy: session.userId,
      closedByName: session.displayName,
      approvedBy: null,
      notes,
    };
    state.zReports.set(till.id, z);
    state.closedSessions.unshift(closedSession);
    state.till = null;
    return { z, backup: null, backupError: null };
  };

  const login = ({ user, pin }: LoginInput): Session => {
    const found = state.users.find((u) => u.tile.id === user || u.session.username === user);
    if (!found) refuse("user_not_found");
    if (found.pin !== pin) refuse("invalid_pin");
    state.session = found.session;
    state.grants = [];
    return found.session;
  };

  const firstRun = (input: FirstRunInput): Session => {
    if (state.setupDone) refuse("setup_already_done");
    state.company = { ...companyFixture, nameFr: input.shopName, taxRegime: input.taxRegime };
    const session: Session = { ...ownerSession, displayName: input.ownerName, lang: input.lang };
    state.users = [
      {
        tile: { ...owner.tile, displayName: input.ownerName, lang: input.lang },
        session,
        pin: input.ownerPin,
      },
    ];
    state.settings["ui.login_lang"] = input.lang;
    state.settings["shop.activity"] = input.activity;
    state.setupDone = true;
    state.session = session;
    return session;
  };

  const override = ({ approverId, pin, permission }: OverrideInput): OverrideGrant => {
    const approver = userOf(approverId) ?? refuse("user_not_found");
    if (approver.tile.id === state.session?.userId) refuse("override_self_not_allowed");
    if (approver.pin !== pin) refuse("invalid_pin");
    if (!approver.session.permissions.includes(permission)) refuse("approver_lacks_permission");
    state.grants.push(permission);
    return {
      approverId,
      approverName: approver.tile.displayName,
      permission,
      grantedAt: NOW,
      discountLimitBp: approver.session.discountLimitBp,
    };
  };

  const setLanguage = (next: Lang): Session => {
    const session = signedIn();
    const user = userOf(session.userId);
    const updated = { ...session, lang: next };
    if (user) {
      user.session = updated;
      user.tile = { ...user.tile, lang: next };
    }
    state.session = updated;
    return updated;
  };

  const search = ({ text, limit, categoryId }: SearchQuery): ProductHit[] => {
    const needle = text.trim().toLowerCase();
    return PRODUCTS.filter(
      (p) =>
        (categoryId === null || p.categoryId === categoryId) &&
        [p.nameAr, p.nameFr, p.nameEn, p.reference ?? "", ...p.barcodes.map((b) => b.barcode)].some((v) =>
          v.toLowerCase().includes(needle),
        ),
    )
      .slice(0, limit ?? 50)
      .map(hitOf);
  };

  const findByCode = (code: string): ScanHit | null => {
    const product = PRODUCTS.find(
      (p) => p.barcodes.some((b) => b.barcode === code) || p.reference === code.toUpperCase(),
    );
    return product ? { product, packQty: 1_000, scale: null } : null;
  };

  const summaryOf = (doc: Document): DocumentSummary => ({
    id: doc.id,
    docType: doc.docType,
    number: doc.number,
    status: doc.status,
    businessDate: doc.businessDate,
    validatedAt: doc.validatedAt,
    totalDue: doc.totalDue,
    userId: doc.userId,
    terminalId: doc.terminalId,
    parentId: doc.parentId,
  });

  return (cmd, args) => {
    switch (cmd) {
      case "app_startup_error": {
        const failure: StartupFailure | null =
          mode === "fatal"
            ? {
                error: { code: "database_corrupt" },
                dataDir: "C:\\data",
                tillOf: null,
                unsent: null,
                mainPcTills: null,
              }
            : null;
        return failure;
      }
      case "app_startup_notices":
        return [] satisfies StartupNotice[];
      case "app_info":
        return appInfoFixture;
      case "app_ready":
        return appInfoFixture.coldStartMs;
      // the regulated staples' board reads the pack's ceilings (none in the fake pack)
      case "datapack_get":
        return { ...packFixture, regulated: { source: "", items: [] } } satisfies Pick<
          Pack,
          "nifLengths" | "wilayas" | "regulated"
        >;
      case "display_get":
        return DISPLAY;
      case "display_show":
      case "sales_record_event":
        return null;
      case "demo_status":
        return { active: false, exists: false, ownerPin: null, cashierPin: null } satisfies DemoStatus;
      case "license_status":
        return state.license;
      case "license_activate": {
        const key = String(args.key ?? "").trim();
        if (!key.startsWith("DZL1.")) refuse("license_invalid");
        state.license = activeStatus;
        return state.license;
      }
      case "support_contact":
        return {
          whatsapp: null,
          url: "https://dzbuild.com/pos/support",
          label: null,
        } satisfies SupportContact;
      case "setup_status":
        return {
          completed: state.setupDone,
          hasOwner: state.users.length > 0,
          terminalCode: TERMINAL,
          loginLang: (state.settings["ui.login_lang"] as Lang | undefined) ?? lang,
        } satisfies SetupStatus;
      case "setup_complete_first_run":
        return firstRun(args.input as FirstRunInput);
      case "auth_list_users":
        return state.users.map((u) => u.tile);
      case "auth_login":
        return login(args.input as LoginInput);
      case "auth_logout":
        state.session = null;
        state.grants = [];
        return null;
      case "auth_session":
        return state.session;
      case "auth_set_language":
        return setLanguage(args.lang as Lang);
      case "auth_manager_override":
        return override(args.input as OverrideInput);
      case "company_get":
        return state.company;
      case "settings_get":
        return state.settings[String(args.key)] ?? null;
      case "settings_set":
        state.settings[String(args.key)] = args.value;
        return args.value;
      case "hardware_get":
        return HARDWARE;
      // The wizard's scale step (M8): no COM port and no device on the fake PC.
      case "hardware_discover":
        return [] satisfies Discovered[];
      case "devices_status":
        return [] satisfies DeviceStatus[];
      // M8: the wizard's templates; the rail and the pack screens read the pack state.
      case "packs_templates":
        return TEMPLATES;
      case "packs_state":
        return packStateOf((state.settings["shop.activity"] as ShopActivity | undefined) ?? "general");
      // M8 (Task 24): the till's keys are the defaults, and the shop has no custom payment type
      // nor BaridPay QR
      case "till_key_map":
        return Object.entries(DEFAULT_KEYS);
      case "payment_types_list":
        return [];
      case "payments_baridpay_qr":
        return null;
      // M8 restaurant: the floor and its editors (no ticket is open on the floor); the table
      // ticket's screen opened from its address shows an open ticket at T1
      case "dining_ticket_get":
        return ticket({
          id: String(args.id),
          tableId: TABLES[0]?.id ?? null,
          waiterId: owner.session.userId,
          waiterName: owner.session.displayName,
          lang,
        });
      case "dining_rooms":
        return [ROOM];
      case "dining_tables":
        return TABLES;
      case "dining_board":
        return TABLES.map((table) => ({ table, state: "free", tickets: [] })) satisfies TableStatus[];
      case "dining_counter":
      case "kitchen_stations":
      case "modifier_groups":
      case "daily_menu_get":
        return [];
      // M8 pharmacy, butcher and catalogue modules (Task 25): the shop has no lot, register
      // entry, cutting template, waste, regulated staple or department key yet
      case "pharmacy_expiry_board":
        return { expired: [], soon: [], expiredCost: 0, soonCost: 0 } satisfies ExpiryBoard;
      case "lots_expiry":
      case "controlled_register_entries":
      case "yield_templates":
      case "regulated_board":
      case "departments_list":
        return [];
      case "waste_day":
        return { rows: [], totalCost: 0 } satisfies WasteDay;
      case "stock_locations":
        return locationsFixture;
      case "catalog_units_list":
        return unitsFixture;
      case "catalog_tax_rates_list":
        return taxRatesFixture;
      case "backup_status":
        return backupStatusFixture;
      case "backup_list":
        return [backupFixture];
      case "backup_mirror_status":
        return {
          folder: null,
          label: null,
          available: false,
          freeBytes: null,
          lastCopyAt: null,
          lastError: null,
        } satisfies MirrorStatus;
      case "backup_drives":
        return [] satisfies RemovableDrive[];
      case "recovery_list":
        return [] satisfies RecoveryBackup[];
      case "updates_status":
        return updateStatusFixture;
      case "reports_dashboard":
        return dashboardFixture;
      case "reports_catalog":
        // the dashboard's shortcuts to the reports: none in the fake shop
        return [] satisfies ReportInfo[];
      case "catalog_categories_list":
        return CATEGORIES;
      case "catalog_price_levels_list":
        return priceLevelsFixture;
      case "products_grid": {
        const categoryId = args.categoryId as string | null;
        return PRODUCTS.filter((p) => categoryId === null || p.categoryId === categoryId).map(hitOf);
      }
      case "products_search":
        return search(args.query as SearchQuery);
      case "products_find_by_code":
        return findByCode(String(args.code));
      case "sales_preview":
        return preview(args.input as PreviewInput);
      case "sales_create":
        return sale(args.request as SaleRequest);
      case "sales_held":
        return [] satisfies HeldCart[];
      case "documents_arabic_guard":
        return null;
      case "documents_find":
        return state.documents.find((d) => d.number === String(args.number).toUpperCase()) ?? null;
      case "documents_list":
        return state.documents.map(summaryOf);
      case "returns_returnable":
        return returnable(String(args.originalId));
      case "returns_quote":
        return quote(args.input as ReturnInput);
      case "returns_create":
        return createReturn(args.request as ReturnRequest);
      case "cash_current":
        return state.till;
      case "cash_open_session": {
        const session = require("cash.open");
        if (state.till) refuse("cash_session_already_open");
        state.till = {
          ...cashSessionFixture,
          id: `s-${state.closedSessions.length + 2}`,
          userId: session.userId,
          openingFloat: Number(args.openingFloat),
        };
        return state.till;
      }
      case "cash_x_report": {
        const x = summary(openTill());
        // a blind close (CSH-004): the expected cash only for whoever may see it
        return signedIn().permissions.includes("cash.view_expected") ? x : { ...x, expectedCash: null };
      }
      case "cash_close_session":
        return close(args.input as CloseInput);
      case "cash_z_report":
        return state.zReports.get(String(args.sessionId)) ?? refuse("cash_session_not_found");
      case "cash_print_z":
        // the fake PC has no receipt printer, like a fresh install
        return refuse("printer_not_configured");
      case "cash_sessions":
        return [...(state.till ? [state.till] : []), ...state.closedSessions];
      case "cash_movements":
        // no paid-in, paid-out or expense in the fake shop
        return [] satisfies CashMovement[];
      // The fake shop is a single PC (M5) with no DZBuild store linked (M7): the shell asks both
      // on every screen (the network pill and banner, the sync dot, the online orders entry).
      case "lan_status":
        return lanStandaloneFixture;
      case "cloud_status":
        return unlinkedStatus;
      case "cloud_notices":
        return [] satisfies CloudNotice[];
      case "cloud_outbox_failed":
        return [] satisfies FailedItem[];
      case "cloud_orders_list":
        return [] satisfies OnlineOrderSummary[];
      // M8 (Tasks 26, 27, 28): no ticket, contract, order, appointment nor round yet, and the
      // operators' float (the general shop runs these modules: `packStateOf`)
      case "rounds_list":
        return [] satisfies Round[];
      case "instalments_list":
        return [] satisfies Plan[];
      case "instalments_reminders":
        return [] satisfies Reminder[];
      case "orders_list":
        return [] satisfies OrderSummary[];
      case "appointments_day":
        return [] satisfies Appointment[];
      case "service_board":
      case "service_uncollected":
        return [] satisfies ServiceSummary[];
      case "flexy_operators":
        return flexyOperators();
      case "flexy_balances":
        return flexyOperators().map((o) => [o.code, o.balance]);
      case "flexy_entries":
        return [] satisfies FloatEntry[];
      // M8 packs: no voucher sold and no price per gram set in the fake shop yet.
      case "vouchers_list":
        return [] satisfies Voucher[];
      case "reports_run":
        return vouchersReport(args.request as ReportRequest);
      case "metal_board":
        return [] satisfies MetalPrice[];
      // the till's checkout asks whether a line is a precious-metal piece: none in the fake shop
      case "jewellery_piece_get":
        return null;
      // M8 Task 30: no employee file nor shift yet; no zakat until the shop enters its nisab, then
      // the zakat of an empty shop is below it.
      case "staff_list":
        return [] satisfies StaffProfile[];
      case "staff_shifts":
        return [] satisfies ShiftRow[];
      case "zakat_preview": {
        const nisab = state.settings["zakat.nisab"];
        if (typeof nisab !== "number") refuse("zakat_nisab_missing");
        const source = state.settings["zakat.nisab_source"];
        return emptyZakat(lang, nisab, typeof source === "string" ? source : "");
      }
      case "zakat_list":
        return [] satisfies ZakatStatement[];
      // Task 21b: no consent recorded yet, no DPO named, the default keys, no data pack installed
      case "privacy_consents":
        return [] satisfies Consent[];
      case "privacy_dpo":
        return null;
      case "datapack_installed":
        return [] satisfies InstalledPack[];
      default:
        throw new Error(`unmocked command ${cmd}`);
    }
  };
}

/** The zakat of the fake shop: nothing counted, below the `nisab` the shop entered. */
function emptyZakat(lang: Lang, nisab: number, nisabSource: string): ZakatPreview {
  const hijriLabel = { ar: "17 ربيع الآخر 1448", fr: "17 rabia al-thani 1448", en: "17 Rabi al-Thani 1448" }[
    lang
  ];
  return {
    day: "2026-09-28",
    hijriDate: { year: 1448, month: 4, day: 17 },
    hijriLabel,
    valuation: "cost",
    stockValue: 0,
    cash: 0,
    bank: 0,
    otherAssets: 0,
    receivables: 0,
    payables: 0,
    otherDebts: 0,
    base: 0,
    nisab,
    nisabSource,
    rateBp: 250,
    zakatDue: 0,
    belowNisab: true,
  };
}

/** The vouchers report the loyalty screen reads for what the shop owes (`gift_vouchers`, nothing
 * sold): the only report the screens of the run ask for. */
function vouchersReport(request: ReportRequest): ReportView {
  if (request.kind !== "gift_vouchers") throw new Error(`unmocked report ${request.kind}`);
  return {
    report: {
      kind: request.kind,
      from: request.from,
      to: request.to,
      table: {
        columns: [
          { key: "code", kind: "code" },
          { key: "voucherKind", kind: "tag", tag: "voucher_kind" },
          { key: "issuedOn", kind: "date" },
          { key: "expiresOn", kind: "date" },
          { key: "initialAmount", kind: "money" },
          { key: "spent", kind: "money" },
          { key: "balance", kind: "money" },
          { key: "voucherStatus", kind: "tag", tag: "voucher_status" },
        ],
        rows: [],
        totals: [null, null, null, null, 0, 0, 0, null],
      },
      chart: null,
      truncated: false,
    },
    title: "",
    subtitle: "",
    labels: [],
    tagLabels: {},
  };
}
