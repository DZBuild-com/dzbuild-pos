import datapack2026 from "../../crates/pos-core/data/dz-datapack-2026.json";
import contractAr from "@/documents/__fixtures__/contract.ar.json";
import contractEn from "@/documents/__fixtures__/contract.en.json";
import contractFr from "@/documents/__fixtures__/contract.fr.json";
import registerAr from "@/documents/__fixtures__/processing-register.ar.json";
import registerEn from "@/documents/__fixtures__/processing-register.en.json";
import registerFr from "@/documents/__fixtures__/processing-register.fr.json";
import routeSheetAr from "@/documents/__fixtures__/route-sheet.ar.json";
import routeSheetEn from "@/documents/__fixtures__/route-sheet.en.json";
import routeSheetFr from "@/documents/__fixtures__/route-sheet.fr.json";
import { ALL_MODULES } from "@/features/packs/modules";
import { useCart } from "@/features/pos/cart-store";
import { formatDZD } from "@/lib/format";
import type { Lang } from "@/lib/i18n";
import type {
  Account,
  Appointment,
  BoardLot,
  Cart,
  ChifaCheck,
  Consent,
  ContractPrint,
  Customer,
  CustomerHit,
  CustomerPrice,
  CutResult,
  DepartmentKey,
  DiningTable,
  DueState,
  ExpiryBoard,
  ExpiryRow,
  FlexyOperator,
  HardwareConfig,
  ImportFile,
  ImportPreview,
  InstalmentQuote,
  JewellerRegisterEntry,
  MetalPrice,
  ModifierGroup,
  Order,
  OrderKind,
  Pack,
  PackState,
  PaymentPreview,
  PaymentType,
  Permission,
  Plan,
  ProcessingRegisterPrint,
  Product,
  ProductExtras,
  ProductHit,
  RegisterEntry,
  RegulatedProduct,
  ReportRequest,
  ReportView,
  Room,
  Round,
  RoundStatus,
  RouteSheetPrint,
  RunPreview,
  Serial,
  ServiceSummary,
  ServiceTicket,
  Session,
  StaffProfile,
  Station,
  Stop,
  TableStatus,
  TemplateInfo,
  Ticket,
  TicketLine,
  TicketSummary,
  VariantGrid,
  Voucher,
  VoucherInfo,
  VoucherTarget,
  WasteDay,
  YieldTemplate,
  ZakatPreview,
} from "@/lib/ipc/types";
import {
  ALL_PERMISSIONS,
  cashierSession,
  companyFixture,
  ownerSession,
  ownerTile,
  settingsValues,
  updateStatusFixture,
} from "./fixtures";
import {
  cashSessionFixture,
  categoriesFixture,
  previewFixture,
  priceLevelsFixture,
  productFixture,
  productHitsFixture,
  taxRatesFixture,
  unitsFixture,
} from "./fixtures-m1";
import { customerFixture, locationsFixture } from "./fixtures-m2";
import { lanStandaloneFixture } from "./fixtures-m5";
import { unlinkedStatus } from "./fixtures-m7";
import { activeStatus } from "./fixtures-m9";
import type { Handler } from "./render";

/** Farid Bouzid's instalment contract (120 000 DA, 30 000 DA down, six dues), as the Rust model
 * serialises it (`cargo test -p pos-docs write_pack_fixtures -- --ignored`). */
export function contractPrint(lang: Lang): ContractPrint {
  const byLang: Record<Lang, unknown> = { ar: contractAr, fr: contractFr, en: contractEn };
  return byLang[lang] as ContractPrint;
}

/** Rachid's round: an invoice of 5 000 DA collected in cash, a delivery note of 10 000 DA still to
 * deliver, as the Rust model serialises it. */
export function routeSheetPrint(lang: Lang): RouteSheetPrint {
  const byLang: Record<Lang, unknown> = { ar: routeSheetAr, fr: routeSheetFr, en: routeSheetEn };
  return byLang[lang] as RouteSheetPrint;
}

/** The processing register of a pharmacy with loyalty and its DPO Nadia Ferhat (Task 21b), as
 * the Rust model serialises it (`cargo test -p pos-docs write_pack_fixtures -- --ignored`). */
export function processingRegisterPrint(lang: Lang): ProcessingRegisterPrint {
  const byLang: Record<Lang, unknown> = { ar: registerAr, fr: registerFr, en: registerEn };
  return byLang[lang] as ProcessingRegisterPrint;
}

// M8 Task 26 (phones and services).

/** An amount as the screen shows it (`formatDZD`: U+202F between thousands in fr and ar). */
export function money(centimes: number, lang: Lang = "fr"): string {
  return formatDZD(centimes, lang);
}

/** M0's owner session holding exactly `permissions`, in `lang`; `"all"` is the owner with every
 * permission, `pharmacy.register` included. */
export function sessionWith(permissions: Permission[] | "all", lang: Lang = "fr"): Session {
  if (permissions === "all") return { ...ownerSession, permissions: [...ALL_PERMISSIONS], lang };
  return { ...ownerSession, isOwner: false, permissions, lang };
}

export function packState(overrides: Partial<PackState> = {}): PackState {
  return {
    activity: "general",
    modules: [],
    tillMode: "retail",
    templateModules: [],
    customised: false,
    ...overrides,
  };
}

/** The customer's latest answers: `true` granted, `false` withdrawn, absent never asked. Given on
 * 12/09/2026 at the counter. */
export function consents(answers: { sms?: boolean; whatsapp?: boolean } = {}): Consent[] {
  return (["sms", "whatsapp"] as const).flatMap((channel) => {
    const granted = answers[channel];
    return granted === undefined
      ? []
      : [
          {
            channel,
            granted,
            source: "counter" as const,
            note: null,
            createdAt: Date.UTC(2026, 8, 12, 9),
            userName: "Sofiane",
          },
        ];
  });
}

export function serviceSummary(overrides: Partial<ServiceSummary> = {}): ServiceSummary {
  return {
    id: "st1",
    number: "RP2026-000001",
    kind: "repair",
    customerName: "Karima Saadi",
    item: "iPhone 11",
    status: "received",
    technicianName: null,
    promisedOn: "2026-10-05",
    readyAt: null,
    daysOpen: 1,
    readyDays: null,
    deposits: 0,
    ...overrides,
  };
}

/** One ticket per open status but `in_progress`: Nassim's Samsung A54 (`st4`) is ready. */
export function serviceBoard(): ServiceSummary[] {
  return [
    serviceSummary(),
    serviceSummary({
      id: "st2",
      number: "RP2026-000002",
      kind: "pressing",
      customerName: "Yacine Brahimi",
      item: "Costume 2 pièces",
      status: "diagnosing",
      daysOpen: 2,
    }),
    serviceSummary({
      id: "st3",
      number: "RP2026-000003",
      customerName: "Lina Mansouri",
      item: "Redmi Note 12",
      status: "waiting_parts",
      technicianName: "Sofiane",
      promisedOn: "2026-09-30",
      daysOpen: 6,
      deposits: 300_000,
    }),
    serviceSummary({
      id: "st4",
      number: "RP2026-000004",
      customerName: "Nassim Ould Ali",
      item: "Samsung A54",
      status: "ready",
      technicianName: "Sofiane",
      promisedOn: "2026-10-01",
      readyAt: Date.UTC(2026, 9, 2, 10),
      daysOpen: 5,
      readyDays: 1,
      deposits: 500_000,
    }),
  ];
}

/** Nassim's Samsung A54 (`st4`, `RP2026-000004`): a screen and its labour planned, 5 000 DA held. */
export function serviceTicket(overrides: Partial<ServiceTicket> = {}): ServiceTicket {
  const status = overrides.status ?? "received";
  const received = {
    status: "received" as const,
    note: null,
    createdAt: Date.UTC(2026, 8, 28, 9),
    userName: "Amine",
  };
  return {
    id: "st4",
    number: "RP2026-000004",
    kind: "repair",
    customerId: "c-nassim",
    customerName: "Nassim Ould Ali",
    item: "Samsung A54",
    brand: "Samsung",
    model: "A54",
    serialNo: "356938035643809",
    accessories: "Chargeur, coque",
    condition: "Écran fissuré",
    fault: "Remplacement écran",
    estimate: 1_500_000,
    promisedOn: "2026-10-01",
    technicianId: "u-tech",
    technicianName: "Sofiane",
    status,
    readyAt: status === "ready" ? Date.UTC(2026, 9, 2, 10) : null,
    closedAt: null,
    documentId: null,
    lang: "fr",
    businessDate: "2026-09-28",
    createdAt: Date.UTC(2026, 8, 28, 9),
    items: [
      {
        id: "si1",
        position: 1,
        productId: "p-screen-a54",
        name: "Écran Samsung A54",
        qty: 1_000,
        unitPrice: null,
      },
      { id: "si2", position: 2, productId: "p-labour", name: "Main-d'œuvre", qty: 1_000, unitPrice: 300_000 },
    ],
    deposits: 500_000,
    events:
      status === "received"
        ? [received]
        : [received, { status, note: null, createdAt: Date.UTC(2026, 9, 2, 10), userName: "Sofiane" }],
    ...overrides,
  };
}

/** The data pack's three operators with the shop's commission and their float. */
export function flexyOperators(): FlexyOperator[] {
  const operator = (code: string, ar: string, name: string, prefixes: string[], balance: number) => ({
    code,
    nameAr: ar,
    nameFr: name,
    nameEn: name,
    prefixes,
    productId: `p-flexy-${code}`,
    commissionBp: 400,
    balance,
  });
  return [
    operator("mobilis", "موبيليس", "Mobilis", ["06"], 1_000_000),
    operator("djezzy", "جازي", "Djezzy", ["07"], 250_000),
    operator("ooredoo", "أوريدو", "Ooredoo", ["05"], 0),
  ];
}

// M8 Task 27 (instalments, customer orders and appointments).

/** What `customers_search` finds for `name`: no debt, no limit. */
export function customerHit(name: string, overrides: Partial<CustomerHit> = {}): CustomerHit {
  return {
    id: `c-${(name.split(" ")[0] ?? name).toLowerCase()}`,
    code: "C00007",
    name,
    customerType: "individual",
    phone: "0661987654",
    wilaya: "16",
    balance: 0,
    creditLimit: null,
    availableCredit: null,
    storeCredit: 0,
    isBlocked: false,
    isFavourite: false,
    archived: false,
    ...overrides,
  };
}

/** Amina Belkacem's file, read in `lang` (her documents and messages). */
export function customerWithLang(lang: Lang): Customer {
  return { ...customerFixture, id: "c-amina", code: "C00009", name: "Amina Belkacem", lang };
}

/** A fridge sold 120 000 DA on six monthly dues of 15 000 DA after 30 000 DA down in cash, whose
 * droit de timbre is 300 DA (what `instalments_quote` answers). */
export function instalmentQuote(): InstalmentQuote {
  return {
    total: 12_000_000,
    down: 3_000_000,
    stampDuty: 30_000,
    downToReceive: 3_030_000,
    financed: 9_000_000,
    dues: [1, 2, 3, 4, 5, 6].map((seq) => ({
      seq,
      dueOn: `2027-${String(seq).padStart(2, "0")}-15`,
      amount: 1_500_000,
    })),
  };
}

/** Farid Bouzid's contract `CT2026-000001` (`pl1`): six dues of 15 000 DA; the first `paidDues`
 * paid, the next one with `partial` paid on it, late by `overdueDays` (else to come). */
export function plan({
  paidDues = 0,
  partial = 0,
  overdueDays = 0,
}: { paidDues?: number; partial?: number; overdueDays?: number } = {}): Plan {
  const dues: DueState[] = [1, 2, 3, 4, 5, 6].map((seq) => {
    const dueOn = `2026-${String(seq + 7).padStart(2, "0")}-15`;
    if (seq <= paidDues) {
      return { seq, dueOn, amount: 1_500_000, paid: 1_500_000, status: "paid", daysLate: 0 };
    }
    if (seq === paidDues + 1) {
      const late = overdueDays > 0;
      return {
        seq,
        dueOn,
        amount: 1_500_000,
        paid: partial,
        status: late ? "overdue" : partial > 0 ? "partial" : "upcoming",
        daysLate: overdueDays,
      };
    }
    return { seq, dueOn, amount: 1_500_000, paid: 0, status: "upcoming", daysLate: 0 };
  });
  const paid = paidDues * 1_500_000 + partial;
  return {
    id: "pl1",
    number: "CT2026-000001",
    customerId: "c-farid",
    customerName: "Farid Bouzid",
    documentId: "d1",
    documentNumber: "TK2026-000050",
    creditEntryId: "le1",
    total: 12_000_000,
    downPayment: 3_000_000,
    financed: 9_000_000,
    count: 6,
    firstDueOn: "2026-08-15",
    dues,
    paid,
    remaining: 9_000_000 - paid,
    guarantees: [
      {
        id: "g1",
        kind: "id_document",
        holderName: "Farid Bouzid",
        reference: "CNI 123456789",
        phone: null,
        note: null,
        createdAt: Date.UTC(2026, 6, 15, 9),
      },
    ],
    businessDate: "2026-07-15",
    createdAt: Date.UTC(2026, 6, 15, 9),
  };
}

/** Amina Belkacem's order `CC2026-000003` (`o1`) of a 60 000 DA sofa, holding `held`. */
export function order({
  held = 0,
  kind = "order",
  status = "open",
}: { held?: number; kind?: OrderKind; status?: Order["status"] } = {}): Order {
  return {
    id: "o1",
    number: "CC2026-000003",
    kind,
    status,
    customerId: "c-amina",
    customerName: "Amina Belkacem",
    dueOn: "2026-10-10",
    total: 6_000_000,
    held,
    reserved: [{ productId: "p-sofa", name: "Canapé 3 places", qty: 1_000 }],
    fulfilledDocumentId: null,
    fulfilledNumber: null,
    lines: [
      {
        productId: "p-sofa",
        description: "Canapé 3 places",
        qty: 1_000,
        unitPrice: 6_000_000,
        totalTtc: 6_000_000,
      },
    ],
    events: [{ status: "open", note: null, createdAt: Date.UTC(2026, 8, 28, 9), userName: "Sofiane" }],
    businessDate: "2026-09-28",
    createdAt: Date.UTC(2026, 8, 28, 9),
  };
}

/** The till's cart for an order: one line worth `total` (as the till's preview prices it) for
 * `customer`, in his language. What `OrderDialog` receives from the till. */
export function cartWith(total: number, customer: Customer): { cart: Cart; total: number } {
  return {
    cart: {
      lines: [
        {
          productId: "p-sofa",
          description: null,
          qty: 1_000,
          unitPrice: null,
          discount: { kind: "none" },
          taxRateId: null,
          note: null,
          expectedUnitPrice: total,
        },
      ],
      priceLevelId: null,
      customerId: customer.id,
      lang: customer.lang ?? "fr",
      note: null,
    },
    total,
  };
}

/** Nadia's (`u1`) day on 28/09/2026: a cut at 09:00 (30 min) and a colour at 11:00 (45 min),
 * Algiers time; 10:00 is free. */
export function appointments(): Appointment[] {
  const at = (hour: number, minute = 0) => Date.UTC(2026, 8, 28, hour - 1, minute);
  const base = {
    customerId: null,
    phone: "0550111222",
    serviceProductId: "p-cut",
    staffUserId: "u1",
    staffName: "Nadia",
    status: "booked" as const,
    note: null,
    createdAt: Date.UTC(2026, 8, 27, 9),
  };
  return [
    { ...base, id: "a1", customerName: "Karima", serviceName: "Coupe", startsAt: at(9), durationMin: 30 },
    {
      ...base,
      id: "a2",
      customerName: "Lina",
      serviceName: "Coloration",
      startsAt: at(11),
      durationMin: 45,
    },
  ];
}

// M8 Task 28 (wholesale and delivery rounds).

/** The prices agreed with customer `c1`: water at 35 DA (list 40 DA, −12,5 %) and flour at
 * 2 400 DA (list 2 500 DA, −4,0 %). */
export function customerPrices(): CustomerPrice[] {
  const agreed = (
    productId: string,
    productName: string,
    reference: string,
    price: number,
    list: number,
  ) => ({
    customerId: "c1",
    productId,
    productName,
    reference,
    price,
    listPrice: list,
    updatedAt: Date.UTC(2026, 8, 28, 9),
  });
  return [
    agreed("p-water", "Eau minérale 1,5 L", "EAU-15", 3_500, 4_000),
    agreed("p-flour", "Farine 25 kg", "FAR-25", 240_000, 250_000),
  ];
}

/** Rachid's round `TR2026-000004` (`r1`, van "Camion 1"): stop 1 an invoice of 5 000 DA to Épicerie
 * Amine, stop 2 a delivery note of 10 000 DA to Supérette Nour. With `collectedFirst`, stop 1 was
 * delivered and paid that much in cash. */
export function round({
  status = "planned",
  collectedFirst = 0,
}: { status?: RoundStatus; collectedFirst?: number } = {}): Round {
  const stop = (overrides: Partial<Stop> & Pick<Stop, "seq">): Stop => ({
    documentId: `d${overrides.seq}`,
    documentNumber: null,
    docType: "invoice",
    customerId: null,
    customerName: null,
    address: null,
    phone: null,
    amountDue: 0,
    status: "pending",
    collected: 0,
    method: null,
    reference: null,
    chequeBank: null,
    chequeDate: null,
    receiptNumber: null,
    note: null,
    ...overrides,
  });
  const paid = collectedFirst > 0;
  return {
    id: "r1",
    number: "TR2026-000004",
    businessDate: "2026-10-03",
    driverName: "Rachid",
    driverUserId: null,
    vehiclePlate: "01234-116-16",
    vanLocationId: "loc-van-1",
    vanLocationName: "Camion 1",
    status,
    note: null,
    startedAt: status === "planned" ? null : Date.UTC(2026, 9, 3, 7),
    closedAt: status === "closed" ? Date.UTC(2026, 9, 3, 16) : null,
    stops: [
      stop({
        seq: 1,
        documentNumber: "FA2026-000012",
        customerId: "c-amine",
        customerName: "Épicerie Amine",
        address: "12 rue Larbi Ben M'hidi, Blida",
        phone: "0550123456",
        amountDue: 500_000,
        status: paid ? "delivered" : "pending",
        collected: collectedFirst,
        method: paid ? "cash" : null,
        receiptNumber: paid && status === "closed" ? "RV2026-C1-000007" : null,
      }),
      stop({
        seq: 2,
        documentNumber: "BL2026-000031",
        docType: "delivery_note",
        customerId: "c-nour",
        customerName: "Supérette Nour",
        address: "Cité 500 logements, Boufarik",
        phone: "0661223344",
        amountDue: 1_000_000,
      }),
    ],
    totalDue: 1_500_000,
    totalCollected: collectedFirst,
    cashCollected: collectedFirst,
  };
}

/** Customer `c1` enrolled with card `2970000000014`: 640 points, a point worth 1 DA. */
export function loyaltyAccount(overrides: Partial<Account> = {}): Account {
  return {
    customerId: "c1",
    card: { code: "2970000000014", customerId: "c1", issuedAt: 1_790_596_800_000, blockedAt: null },
    balance: 640,
    expiringSoon: 0,
    nextExpiry: null,
    pointValue: 100,
    redeemable: true,
    ...overrides,
  };
}

/** Gift voucher BA2026-000001 of 5 000 DA sold in cash (50 DA of duty charged), valid a year. */
export function voucher(overrides: Partial<Voucher> = {}): Voucher {
  return {
    id: "v1",
    code: "BA2026-000001",
    barcode: "2982600000014",
    kind: "gift",
    initialAmount: 500_000,
    balance: 500_000,
    status: "active",
    issuedOn: "2026-09-28",
    expiresOn: "2027-09-28",
    customerId: null,
    buyerName: "Karim",
    sourceDocumentId: null,
    stampDuty: 5_000,
    stampCharged: 5_000,
    ...overrides,
  };
}

/** The board: gold 750 ‰ at 14 500 DA the gram, gold 916 ‰ at 17 700 DA, silver 925 ‰ at 260 DA. */
export function metalBoard(): MetalPrice[] {
  return [
    {
      metal: "gold",
      titrePermille: 750,
      pricePerGram: 1_450_000,
      effectiveAt: 1_790_596_800_000,
      userName: "Amine Benali",
    },
    {
      metal: "gold",
      titrePermille: 916,
      pricePerGram: 1_770_000,
      effectiveAt: 1_790_596_800_000,
      userName: "Amine Benali",
    },
    {
      metal: "silver",
      titrePermille: 925,
      pricePerGram: 26_000,
      effectiveAt: 1_790_510_400_000,
      userName: "Amine Benali",
    },
  ];
}

/** Sofiane Kaci (`u2`), cashier since March 2025: 45 000 DA a month, 1 % on his sales, 10 000 DA
 * of advances still owed. */
export function staffProfile(overrides: Partial<StaffProfile> = {}): StaffProfile {
  return {
    userId: "u2",
    displayName: "Sofiane Kaci",
    roleName: "Caissier",
    active: true,
    phone: "0550123456",
    hireDate: "2025-03-01",
    monthlySalary: 4_500_000,
    commissionBp: 100,
    note: null,
    owed: 1_000_000,
    ...overrides,
  };
}

/** The zakat of 28 September 2026 (17 Rabi' al-Akhir 1448), stock at cost: a base of 3 330 000 DA
 * above the nisab of 2 295 000 DA the shop entered (noted "MARW, 1447"), 83 250 DA due (2.5 %).
 * The Hijri label is the backend's, in Arabic. */
export function zakatPreview(overrides: Partial<ZakatPreview> = {}): ZakatPreview {
  return {
    day: "2026-09-28",
    hijriDate: { year: 1448, month: 4, day: 17 },
    hijriLabel: "17 ربيع الآخر 1448",
    valuation: "cost",
    stockValue: 320_000_000,
    cash: 15_000_000,
    bank: 50_000_000,
    otherAssets: 0,
    receivables: 8_000_000,
    payables: 60_000_000,
    otherDebts: 0,
    base: 333_000_000,
    nisab: 229_500_000,
    nisabSource: "MARW, 1447",
    rateBp: 250,
    zakatDue: 8_325_000,
    belowNisab: false,
    ...overrides,
  };
}

/** Task 17's default till keys (`till::DEFAULT_KEYS`), in its order. */
const TILL_KEYS: ReadonlyArray<readonly [string, string]> = [
  ["help", "F1"],
  ["search", "F2"],
  ["customer", "F3"],
  ["price_check", "F4"],
  ["discount", "F5"],
  ["hold", "F6"],
  ["held", "F7"],
  ["return", "F8"],
  ["drawer", "F9"],
  ["repeat_last", "F10"],
  ["checkout", "F12"],
  ["menu", "Escape"],
  ["floor", "Ctrl+1"],
  ["flexy", "Ctrl+2"],
  ["gift_voucher", "Ctrl+3"],
  ["serial", "Ctrl+4"],
  ["department_keys", "Ctrl+5"],
];

/** What `till_key_map` answers: every action with its key, the defaults with `overrides`. */
export function keyMap(overrides: Record<string, string> = {}): Array<[string, string]> {
  return TILL_KEYS.map(([action, key]) => [action, overrides[action] ?? key]);
}

/** Entry 1 of 2026: an 18-carat ring of 4.250 g sold to Samira Belkacem. */
export function jewellerEntry(overrides: Partial<JewellerRegisterEntry> = {}): JewellerRegisterEntry {
  return {
    year: 2026,
    seq: 1,
    businessDate: "2026-09-28",
    documentNumber: "TK-C1-2026-000042",
    nature: "Bague or 18 carats",
    pieces: 1,
    weightMg: 4_250,
    titrePermille: 750,
    price: 6_662_500,
    buyerName: "Samira Belkacem",
    buyerIdKind: "cni",
    buyerIdNumber: "109876543210987654",
    buyerAddress: "12 rue Larbi Ben M'hidi, Oran",
    userName: "Amine Benali",
    ...overrides,
  };
}

export function templateInfos(): TemplateInfo[] {
  return [
    {
      activity: "general",
      release: "mvp",
      modules: ["department_keys", "regulated_prices"],
      tillMode: "retail",
      categories: [{ ar: "عام", fr: "Général", en: "General" }],
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
}

/** This PC's hardware file on a fresh install (version 2, every section empty): no printer, no
 * scale, a scanner in US keyboard emulation. */
export function hardwareConfig(overrides: Partial<HardwareConfig> = {}): HardwareConfig {
  return {
    version: 2,
    printers: [],
    receiptPrinterId: null,
    receipt: {
      printByDefault: true,
      openDrawerOnCash: true,
      drawerBeforePrint: false,
      logo: true,
      secondLang: null,
    },
    scanner: { layout: "us" },
    labelPrinters: [],
    labelPrinterId: null,
    scale: null,
    ...overrides,
  };
}

export function diningTable(overrides: Partial<DiningTable> = {}): DiningTable {
  return {
    id: "tb1",
    roomId: "r1",
    code: "T1",
    seats: 4,
    shape: "square",
    gridCol: 0,
    gridRow: 0,
    archived: false,
    ...overrides,
  };
}

export function ticketSummary(overrides: Partial<TicketSummary> = {}): TicketSummary {
  return {
    id: "t1",
    orderNo: null,
    mode: "dine_in",
    tableId: "tb1",
    tableCode: "T1",
    label: null,
    covers: 3,
    waiterName: "Sofiane",
    totalTtc: 180_000,
    status: "open",
    openedAt: 1_790_596_800_000,
    minutesOpen: 12,
    lastKitchenAt: 1_790_597_100_000,
    minutesSinceKitchen: 7,
    unprintedKitchen: 0,
    unsentLines: 0,
    ...overrides,
  };
}

/** A room of three tables on its first row: T1 occupied by ticket `t1` (3 covers, 12 min, its
 * last kitchen ticket 7 min ago, 1 800 DA), T2 and T3 free. */
export function board(): TableStatus[] {
  return [
    { table: diningTable(), state: "occupied", tickets: [ticketSummary()] },
    { table: diningTable({ id: "tb2", code: "T2", gridCol: 1 }), state: "free", tickets: [] },
    { table: diningTable({ id: "tb3", code: "T3", gridCol: 2, seats: 6 }), state: "free", tickets: [] },
  ];
}

/** The counter: takeaway order 12 and a delivery (order 13) to an address. */
export function counter(): TicketSummary[] {
  return [
    ticketSummary({
      id: "t12",
      orderNo: 12,
      mode: "takeaway",
      tableId: null,
      tableCode: null,
      covers: 0,
      totalTtc: 65_000,
      minutesOpen: 4,
      lastKitchenAt: null,
      minutesSinceKitchen: null,
    }),
    ticketSummary({
      id: "t13",
      orderNo: 13,
      mode: "delivery",
      tableId: null,
      tableCode: null,
      covers: 0,
      totalTtc: 120_000,
      minutesOpen: 9,
      lastKitchenAt: null,
      minutesSinceKitchen: null,
    }),
  ];
}

/** The two tables of room `r1` as the room editor reads them: T1 at (0, 0), T2 at (1, 0). */
export function tables(): DiningTable[] {
  return [diningTable(), diningTable({ id: "tb2", code: "T2", gridCol: 1 })];
}

/** A forced "Cuisson" group: exactly one of three options. */
export function modifierGroups(): ModifierGroup[] {
  const option = (id: string, nameFr: string, nameAr: string, nameEn: string, sortOrder: number) => ({
    id,
    groupId: "mg-cuisson",
    nameAr,
    nameFr,
    nameEn,
    productId: null,
    sortOrder,
    archived: false,
  });
  return [
    {
      id: "mg-cuisson",
      nameAr: "الطهي",
      nameFr: "Cuisson",
      nameEn: "Cooking",
      minSelect: 1,
      maxSelect: 1,
      sortOrder: 1,
      archived: false,
      options: [
        option("mo-rare", "Saignant", "نصف مطهو", "Rare", 1),
        option("mo-medium", "À point", "متوسط", "Medium", 2),
        option("mo-done", "Bien cuit", "مطهو جيدا", "Well done", 3),
      ],
    },
  ];
}

/** M1b's grid product (`productHitsFixture[0]`) with other names and `price` as its sell price. */
export function productHit({
  price,
  ...overrides
}: Partial<ProductHit> & { price?: number } = {}): ProductHit {
  const base = productHitsFixture[0]!;
  return { ...base, ...overrides, sellPrice: price ?? overrides.sellPrice ?? base.sellPrice };
}

export function ticketLine(overrides: Partial<TicketLine> = {}): TicketLine {
  return {
    id: "l1",
    position: 1,
    productId: "p-brochettes",
    name: "Brochettes",
    cartLine: {
      productId: "p-brochettes",
      description: null,
      qty: 2_000,
      unitPrice: null,
      discount: { kind: "none" },
      taxRateId: null,
      note: null,
      expectedUnitPrice: null,
    },
    qty: 2_000,
    sentQty: 0,
    seat: null,
    course: null,
    parentLineId: null,
    modifierOptionId: null,
    status: "active",
    voidReason: null,
    compReason: null,
    ...overrides,
  };
}

/** A table ticket; `lines` is its lines or how many Brochettes lines it has (one by default),
 * `sent` marks them all received by the kitchen. */
export function ticket({
  lines = 1,
  sent = false,
  ...overrides
}: Omit<Partial<Ticket>, "lines"> & { lines?: number | TicketLine[]; sent?: boolean } = {}): Ticket {
  const list =
    typeof lines === "number"
      ? Array.from({ length: lines }, (_, i) =>
          ticketLine(i === 0 ? {} : { id: `l${i + 1}`, position: i + 1 }),
        )
      : lines;
  return {
    id: "t1",
    mode: "dine_in",
    orderNo: null,
    businessDate: "2026-09-28",
    tableId: "tb1",
    tableCode: "T1",
    label: null,
    covers: 2,
    waiterId: "u-owner",
    waiterName: "Sofiane",
    customerId: null,
    deliveryAddress: null,
    priceLevelId: null,
    lang: "fr",
    note: null,
    status: "open",
    totalTtc: 180_000,
    parentId: null,
    documentId: null,
    openedAt: 1_790_596_800_000,
    billedAt: null,
    closedAt: null,
    lines: sent ? list.map((l) => ({ ...l, sentQty: l.qty })) : list,
    ...overrides,
  };
}

export function stationList(): Station[] {
  return [
    {
      id: "st-kitchen",
      code: "CUISINE",
      nameAr: "المطبخ",
      nameFr: "Cuisine",
      nameEn: "Kitchen",
      sortOrder: 1,
      archived: false,
      categoryIds: ["cat-plats"],
    },
    {
      id: "st-bar",
      code: "BAR",
      nameAr: "البار",
      nameFr: "Bar",
      nameEn: "Bar",
      sortOrder: 2,
      archived: false,
      categoryIds: ["cat-boissons"],
    },
  ];
}

export function registerEntry(overrides: Partial<RegisterEntry> = {}): RegisterEntry {
  return {
    id: "r1",
    number: "2026-000001",
    businessDate: "2026-09-28",
    documentId: "d1",
    documentNumber: "TK-C1-2026-000042",
    lineId: "dl1",
    productId: "p-tramadol",
    productName: "Tramadol 50 mg",
    qty: 1_000,
    prescriberName: "Dr Benali",
    prescriberRef: "CNOM 1234",
    prescriptionNo: "0457",
    prescriptionDate: "2026-09-27",
    patientName: "M. Haddad",
    patientRef: null,
    createdAt: 1_790_596_800_000,
    userName: "Amine Benali",
    ...overrides,
  };
}

/** An 18,5 kg lamb carcass at 25 900 DA cut into 17,2 kg (6 kg of leg, 6,2 kg of shoulder, 5 kg of
 * chops): 1,3 kg of loss, 7,03 %. */
export function yieldPreview(overrides: Partial<RunPreview> = {}): RunPreview {
  const cut = (
    productId: string,
    name: string,
    [qty, expectedQty, yieldBp]: [number, number, number],
    [cost, unitCost]: [number, number],
  ): CutResult => ({ productId, name, qty, expectedQty, yieldBp, cost, unitCost });
  const cuts: CutResult[] = [
    cut("p-gigot", "Gigot", [6_000, 6_105, 3_243], [1_008_000, 168_000]),
    cut("p-epaule", "Épaule", [6_200, 5_550, 3_351], [868_000, 140_000]),
    cut("p-cotes", "Côtelettes", [5_000, 4_625, 2_702], [714_000, 142_800]),
  ];
  return { inputCost: 2_590_000, cuts, outputQty: 17_200, lossQty: 1_300, lossBp: 703, ...overrides };
}

/** "Agneau entier": a lamb carcass cut into leg (33 %), shoulder (30 %) and chops (25 %). */
export function yieldTemplates(): YieldTemplate[] {
  return [
    {
      id: "yt-agneau",
      sourceProductId: "p-agneau",
      name: "Agneau entier",
      cuts: [
        { productId: "p-gigot", expectedBp: 3_300 },
        { productId: "p-epaule", expectedBp: 3_000 },
        { productId: "p-cotes", expectedBp: 2_500 },
      ],
      archived: false,
    },
  ];
}

const CUT_NAMES: Record<string, [ar: string, fr: string, en: string]> = {
  "p-agneau": ["ذبيحة خروف", "Carcasse d'agneau", "Lamb carcass"],
  "p-gigot": ["فخذ", "Gigot", "Leg"],
  "p-epaule": ["كتف", "Épaule", "Shoulder"],
  "p-cotes": ["ضلوع", "Côtelettes", "Chops"],
};

/** `products_get` of the lamb template's carcass and cuts: sold by the kilogram. */
export function cutProduct(id: string): Product {
  const [nameAr, nameFr, nameEn] = CUT_NAMES[id] ?? ["منتج", "Produit", "Product"];
  return {
    ...productFixture,
    id,
    reference: null,
    nameAr,
    nameFr,
    nameEn,
    unitCode: "kg",
    unitDecimals: 3,
    isWeighed: true,
    barcodes: [],
  };
}

/** Today's waste: 2,5 kg of tomatoes broken and 12 baguettes eaten by the staff, 1 220 DA at cost. */
export function wasteDay(): WasteDay {
  return {
    rows: [
      {
        day: "2026-09-28",
        productId: "p-tomates",
        name: "Tomates",
        qty: 2_500,
        reason: "damage",
        cost: 20_000,
      },
      {
        day: "2026-09-28",
        productId: "p-pain",
        name: "Baguette",
        qty: 12_000,
        reason: "own_use",
        cost: 102_000,
      },
    ],
    totalCost: 122_000,
  };
}

/** A lot of the expiry board (Amoxicilline at the shop) with its value at cost. */
function boardLot(lot: Partial<ExpiryRow>, cost: number): BoardLot {
  return {
    lot: {
      productId: "p-amox",
      nameAr: "أموكسيسيلين 500 ملغ",
      nameFr: "Amoxicilline 500 mg",
      nameEn: "Amoxicillin 500 mg",
      lotId: "lot-a1",
      lotNumber: "A1",
      expiryDate: "2026-09-20",
      locationId: "00000000-0000-7000-8000-000000000301",
      qty: 30_000,
      daysLeft: -8,
      status: "expired",
      ...lot,
    },
    cost,
  };
}

/** Two expired lots worth 12 450 DA at cost, and a syrup expiring in twelve days. */
export function expiryBoard(): ExpiryBoard {
  return {
    expired: [
      boardLot({}, 1_050_000),
      boardLot(
        {
          productId: "p-doli",
          nameAr: "دوليبران 1 غ",
          nameFr: "Doliprane 1 g",
          nameEn: "Doliprane 1 g",
          lotId: "lot-d7",
          lotNumber: "D7",
          expiryDate: "2026-09-25",
          qty: 12_000,
          daysLeft: -3,
        },
        195_000,
      ),
    ],
    soon: [
      boardLot(
        {
          productId: "p-sirop",
          nameAr: "شراب السعال",
          nameFr: "Sirop toux sèche",
          nameEn: "Dry cough syrup",
          lotId: "lot-s3",
          lotNumber: "S3",
          expiryDate: "2026-10-10",
          qty: 8_000,
          daysLeft: 12,
          status: "soon",
        },
        360_000,
      ),
    ],
    expiredCost: 1_245_000,
    soonCost: 360_000,
  };
}

/** `chifa-2026.xlsx`, staged by M2's import dialog: one sheet of 120 rows under its header. */
export function chifaFile(): ImportFile {
  return {
    token: "t-chifa",
    fileName: "chifa-2026.xlsx",
    format: "xlsx",
    encoding: null,
    delimiter: null,
    sheets: [{ name: "Liste", rows: 121 }],
  };
}

/** The first rows of the Chifa list as M2's reader shows them. */
export function chifaPreview(): ImportPreview {
  return {
    headerRow: 0,
    headers: ["Code", "Désignation", "Tarif de référence", "Remboursable"],
    sample: [
      ["6130001112223", "Amoxicilline 500 mg", "245,00", "oui"],
      ["6130001112224", "Doliprane 1 g", "98,50", "oui"],
    ],
    fields: [],
    suggested: [null, null, null, null],
  };
}

/** The dry run of the Chifa list: 120 rows, `problems` of them refused (row 14 first). */
export function chifaCheck({ problems = 0 }: { problems?: number } = {}): ChifaCheck {
  return {
    rows: 120,
    valid: 120 - problems,
    matched: 85,
    problems: Array.from({ length: problems }, (_, i) => ({
      row: 14 + i,
      column: "referencePrice",
      code: "number_invalid",
    })),
  };
}

// Task 24: the till's pack extensions.

/** The ticket's total the till's preview answers (`cartWith` sets it) and the customer file
 * `customers_get` answers. */
let tillTotal = 30_000;
let tillCustomer: Customer | null = null;

/** A variant model of the grid (SAL-023): its tile opens the size × colour grid. */
export const oxfordShirt: ProductHit = {
  id: "p-oxford",
  reference: "CHEM-OXF",
  nameAr: "قميص أكسفورد",
  nameFr: "Chemise Oxford",
  nameEn: "Oxford shirt",
  kind: "variant_parent",
  categoryId: "c-grocery",
  unitCode: "pc",
  sellPrice: 350_000,
  barcode: null,
  stock: 5_000,
  image: null,
  archived: false,
  availableAtTill: true,
};

/** The backend's preview of the cart sent: its lines echoed, the ticket at `tillTotal`, the
 * tenders typed counted as paid, no droit de timbre (the tests pay by card, voucher, transfer). */
function tillPreview(args: Record<string, unknown>): PaymentPreview {
  const input = args.input as {
    cart: { lines: { productId: string | null; qty: number; unitPrice: number | null }[] };
    tenders: { amount: number }[];
  };
  const base = previewFixture(tillTotal, 0, 0);
  const template = base.cart.lines[0]!;
  const paid = input.tenders.reduce((sum, t) => sum + t.amount, 0);
  return {
    cart: {
      ...base.cart,
      lines: input.cart.lines.map((l, i) => ({
        ...template,
        lineNo: i + 1,
        productId: l.productId,
        qty: l.qty,
        unitPrice: l.unitPrice ?? template.unitPrice,
      })),
    },
    settlement: {
      ...base.settlement,
      cashDue: tillTotal,
      cashTendered: 0,
      paidTotal: paid,
      missing: Math.max(0, tillTotal - paid),
      change: 0,
    },
  };
}

function productExtras(productId: string, trackSerials = false): ProductExtras {
  return {
    productId,
    brand: null,
    supplierRef: null,
    altRefs: null,
    tags: null,
    attributes: {},
    minPrice: null,
    trackLots: false,
    trackSerials,
    preferredSupplierId: null,
    warrantyMonths: null,
  };
}

/** The handlers M1b's sale-screen tests mock (the cashier's session, the open cash session, the
 * shop, the grid with a variant model, the categories, the preview) with `auth_session` callable
 * on its own. */
export function cashierTill() {
  return {
    auth_session: () => cashierSession,
    auth_list_users: () => [ownerTile],
    company_get: () => ({ ...companyFixture, defaultDocLang: "fr" }),
    settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)] ?? null,
    hardware_get: () => hardwareConfig(),
    cash_current: () => cashSessionFixture,
    catalog_categories_list: () => categoriesFixture,
    catalog_price_levels_list: () => priceLevelsFixture,
    catalog_tax_rates_list: () => taxRatesFixture,
    products_grid: () => [...productHitsFixture, oxfordShirt],
    products_search: () => productHitsFixture.slice(0, 1),
    products_find_by_code: () => null,
    products_extras_get: ({ id }: Record<string, unknown>) => productExtras(String(id)),
    customers_get: () => tillCustomer,
    sales_preview: tillPreview,
    sales_held: () => [],
  };
}

/** A phone tracked by its IMEI, as the scanner finds it. */
export function phoneScanHit() {
  return {
    product: {
      id: "p-a54",
      reference: "A54",
      nameAr: "سامسونغ A54",
      nameFr: "Samsung A54",
      nameEn: "Samsung A54",
      categoryId: null,
      unitId: "u-pc",
      unitCode: "pc",
      unitDecimals: 0,
      taxRateId: "t-19",
      taxRateBp: 1_900,
      kind: "simple" as const,
      parentId: null,
      sellPrice: 6_000_000,
      costPrice: 0,
      minStock: 0,
      trackStock: true,
      allowNegativeStock: false,
      isWeighed: false,
      availableAtTill: true,
      plu: null,
      image: null,
      archived: false,
      barcodes: [],
      prices: [],
      components: [],
      updatedAt: 0,
    },
    packQty: 1_000,
    scale: null,
  };
}

/** `products_extras_get` of a phone: its serials are tracked. */
export function phoneExtras(): ProductExtras {
  return productExtras("p-a54", true);
}

/** The cart of M1b's store with one line, the preview at `total` and, when given, its customer. */
export function tillCart(total: number, customer: Customer | null = null): void {
  tillTotal = total;
  tillCustomer = customer;
  const cart = useCart.getState();
  cart.clear("fr");
  cart.addProduct({
    productId: "p-milk",
    qty: 1_000,
    display: { name: "Lait Candia 1 L", unitCode: "pc", unitDecimals: 0, price: total },
  });
  if (customer) cart.setCustomer(customer.id);
}

/** A customer file whose language is `lang` (UX-004). */
export function customerIn(lang: Lang): Customer {
  return { ...customerFixture, lang };
}

export function departmentKeys(): DepartmentKey[] {
  return [
    {
      id: "dk-veg",
      productId: "op-veg",
      nameAr: "خضر",
      nameFr: "Légumes",
      nameEn: "Vegetables",
      categoryId: null,
      taxRateId: "t-0",
      unitId: "u-kg",
      color: "#1B7F3B",
      sortOrder: 1,
      archived: false,
    },
    {
      id: "dk-fruit",
      productId: "op-fruit",
      nameAr: "فواكه",
      nameFr: "Fruits",
      nameEn: "Fruit",
      categoryId: null,
      taxRateId: "t-0",
      unitId: "u-kg",
      color: null,
      sortOrder: 2,
      archived: false,
    },
  ];
}

/** Two phones in stock; the first IMEI passes its check digit, the second too. */
export function serialsInStock(): Serial[] {
  return ["356938035643809", "490154203237518"].map((serialNo, i) => ({
    id: `sn-${i + 1}`,
    productId: "p-a54",
    serialNo,
    status: "in_stock" as const,
    locationId: null,
    receivedDocumentId: null,
    saleLineId: null,
    updatedAt: 0,
  }));
}

/** The Oxford shirt in sizes S and M, blue and white, by size (`rowAxis: "Taille"`) or by colour;
 * M white is out of stock. */
export function variantGrid({ rowAxis = "Taille" }: { rowAxis?: string } = {}): VariantGrid {
  const bySize = rowAxis !== "Couleur";
  const cell = (variantId: string, size: string, colour: string, onHand: number) => ({
    variantId,
    rowValue: bySize ? size : colour,
    colValue: bySize ? colour : size,
    reference: null,
    barcode: null,
    onHand,
    price: 350_000,
  });
  return {
    parentId: "p-oxford",
    rowAxis: bySize ? "Taille" : "Couleur",
    colAxis: bySize ? "Couleur" : "Taille",
    rows: bySize ? ["S", "M"] : ["Bleu", "Blanc"],
    cols: bySize ? ["Bleu", "Blanc"] : ["S", "M"],
    cells: [
      cell("v-s-bleu", "S", "Bleu", 2_000),
      cell("v-s-blanc", "S", "Blanc", 4_000),
      cell("v-m-bleu", "M", "Bleu", 3_000),
      cell("v-m-blanc", "M", "Blanc", 0),
    ],
    otherAxes: [],
  };
}

export function topUpOperators(): FlexyOperator[] {
  const operator = (code: string, name: string, nameAr: string, prefix: string): FlexyOperator => ({
    code,
    nameAr,
    nameFr: name,
    nameEn: name,
    prefixes: [prefix],
    productId: `svc-${code}`,
    commissionBp: 300,
    balance: 1_000_000,
  });
  return [
    operator("mobilis", "Mobilis", "موبيليس", "06"),
    operator("djezzy", "Djezzy", "جازي", "07"),
    operator("ooredoo", "Ooredoo", "أوريدو", "05"),
  ];
}

/** Prélèvement CCP (core Task 16), riding a transfer with its debit reference. */
export function paymentTypes(): PaymentType[] {
  return [
    {
      id: "00000000-0000-7000-8000-000000000601",
      code: "PRLV",
      nameAr: "اقتطاع CCP",
      nameFr: "Prélèvement CCP",
      nameEn: "CCP direct debit",
      baseMethod: "transfer",
      requiresReference: true,
      sortOrder: 1,
      archived: false,
    },
  ];
}

/** `vouchers_describe` of a gift voucher (`kind` and `code`), a deposit (`number`) or the
 * customer's store credit. */
export function voucherInfo({
  kind,
  code = "BA2026-000001",
  number = "CC2026-000001",
  balance,
  customerId = null,
}: {
  kind: VoucherTarget["kind"];
  code?: string;
  number?: string;
  balance: number;
  customerId?: string | null;
}): VoucherInfo {
  const target: VoucherTarget =
    kind === "gift_voucher"
      ? { kind, code }
      : kind === "deposit"
        ? { kind, ownerType: "customer_order", number }
        : { kind: "store_credit" };
  return { target, balance, customerId };
}

// Task 32: the screen smoke.

/** The restaurant's one room, three columns by two rows. */
export function diningRoom(): Room {
  return {
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
}

/** The data pack the app ships (`crates/pos-core/data`): its regulated staples, nisab, notes. */
export function datapack(): Pack {
  return datapack2026 as unknown as Pack;
}

/** Subsidised milk in sachets linked to its ceiling of 25 DA, sold at 25 DA. */
export function regulatedProducts(): RegulatedProduct[] {
  return [
    {
      productId: "p-lait-sachet",
      nameAr: "حليب الأكياس 1 ل",
      nameFr: "Lait en sachet 1 L",
      nameEn: "Milk in a sachet 1 L",
      code: "milk_sachet_1l",
      ceiling: 2_500,
      price: 2_500,
      priceTtc: 2_500,
      above: false,
    },
  ];
}

/** A report as `reports_run` answers it: the kind asked, the period, one column of text, no row. */
export function emptyReport(request: ReportRequest): ReportView {
  return {
    report: {
      kind: request.kind,
      from: "2026-09-28",
      to: "2026-09-28",
      table: { columns: [{ key: "code", kind: "text" }], rows: [], totals: [null] },
      chart: null,
      truncated: false,
    },
    title: "",
    subtitle: "",
    labels: [],
    tagLabels: {},
  };
}

/**
 * The backend of every M8 screen (and M9's licence screens) for the screen smoke, in `lang`: the
 * pack state with every module on, each list with one or two rows of the fixtures above, a valid
 * licence, and M0/M1b's own answers for the shell. Every command a screen of the smoke calls is
 * here: `m8-smoke.test.tsx` names any other.
 */
export function mockEveryPack(lang: Lang): Record<string, Handler> {
  const owner = sessionWith("all", lang);
  return {
    // M0, M1b, M5, M9: the shell, the till's parts and this PC's licence
    ...cashierTill(),
    auth_session: () => owner,
    auth_list_users: () => [ownerTile],
    license_status: () => activeStatus,
    lan_status: () => lanStandaloneFixture,
    till_key_map: () => keyMap(),
    display_show: () => null,
    demo_status: () => ({ active: false, exists: false, ownerPin: null, cashierPin: null }),
    backup_status: () => ({ lastAuto: null, newestAt: null, stale: false, databaseDamaged: false }),
    cloud_status: () => unlinkedStatus,
    updates_status: () => updateStatusFixture,
    devices_status: () => [],
    datapack_get: () => datapack(),
    datapack_installed: () => [
      { id: "dz-2026.1", version: 1, validFrom: "2026-01-01", fileName: "dz-datapack-2026.dzpack" },
    ],
    catalog_units_list: () => unitsFixture,
    reports_run: ({ request }) => emptyReport(request as ReportRequest),
    // Task 22: every module on, the shop's activity and its template
    packs_state: () =>
      packState({
        modules: [...ALL_MODULES],
        tillMode: "mixed",
        templateModules: ["restaurant"],
        customised: true,
      }),
    packs_templates: () => templateInfos(),
    // Task 23: the restaurant
    dining_rooms: () => [diningRoom()],
    dining_board: () => board(),
    dining_counter: () => counter(),
    dining_tables: () => tables(),
    dining_ticket_get: () => ticket({ lines: 2 }),
    daily_menu_get: () => [],
    modifiers_for_product: () => modifierGroups(),
    modifier_groups: () => modifierGroups(),
    kitchen_stations: () => stationList(),
    // Task 25: the pharmacy, the butcher, the catalogue's modules
    pharmacy_expiry_board: () => expiryBoard(),
    imports_preview: () => chifaPreview(),
    chifa_check: () => chifaCheck(),
    controlled_register_entries: () => [registerEntry()],
    yield_templates: () => yieldTemplates(),
    yield_preview: () => yieldPreview(),
    products_get: ({ id }) => cutProduct(String(id)),
    waste_day: () => wasteDay(),
    departments_list: () => departmentKeys(),
    regulated_board: () => regulatedProducts(),
    // Task 26: repairs and services, flexy
    service_board: () => serviceBoard(),
    service_get: () => serviceTicket(),
    service_uncollected: () => [serviceSummary({ status: "ready" })],
    privacy_consents: () => consents({ sms: true }),
    flexy_operators: () => flexyOperators(),
    flexy_balances: () => flexyOperators().map((o) => [o.code, o.balance]),
    flexy_entries: () => [],
    // Task 27: instalments, customer orders, appointments
    instalments_list: () => [plan({ paidDues: 1, overdueDays: 5 })],
    instalments_get: () => plan({ paidDues: 2 }),
    instalments_reminders: () => [],
    orders_list: () => [],
    orders_get: () => order({ held: 1_200_000 }),
    deposits_list: () => [],
    appointments_day: () => appointments(),
    // Task 28: wholesale and delivery rounds
    rounds_list: () => [round()],
    rounds_get: () => round(),
    stock_locations: () => locationsFixture,
    wholesale_prices: () => customerPrices(),
    // Task 29: loyalty, vouchers, jewellery
    loyalty_account: () => loyaltyAccount(),
    loyalty_entries: () => [],
    vouchers_list: () => [voucher()],
    metal_board: () => metalBoard(),
    jeweller_register_entries: () => [jewellerEntry()],
    // Task 30: staff, zakat and the settings
    staff_list: () => [staffProfile()],
    staff_entries: () => [],
    staff_shifts: () => [],
    zakat_preview: () => zakatPreview(),
    zakat_list: () => [],
    payment_types_list: () => paymentTypes(),
    payments_baridpay_qr: () => null,
  };
}
