import type {
  CollectQuote,
  CountLineView,
  Customer,
  CustomerHit,
  Dashboard,
  EntryPage,
  Expense,
  ExpenseCategoryInfo,
  ExpiryRow,
  ImportFile,
  ImportJob,
  ImportPreview,
  LedgerEntry,
  LevelRow,
  ProductExtras,
  Promotion,
  PurchaseSummary,
  RemainingLine,
  ReportView,
  Statement,
  StockCount,
  StockLocation,
  Supplier,
} from "@/lib/ipc/types";

/** Karim Benali, a reseller who owes 3 000 DA on a 5 000 DA limit. */
export const customerFixture: Customer = {
  id: "c-karim",
  code: "C00001",
  name: "Karim Benali",
  customerType: "reseller",
  phone: "0550123456",
  phone2: null,
  email: null,
  address: "Cité 500 logements, Bab Ezzouar",
  wilaya: "16",
  nif: "000116009876543",
  nis: null,
  rc: "16/00-1234567B21",
  ai: null,
  nin: null,
  legalForm: null,
  activity: null,
  vatExempt: false,
  vatExemptRef: null,
  idsVerifiedAt: null,
  priceLevelId: null,
  discountBp: 0,
  creditLimit: 500_000,
  balance: 300_000,
  availableCredit: 200_000,
  storeCredit: 0,
  lang: "fr",
  notes: null,
  tags: null,
  isBlocked: false,
  isFavourite: true,
  marketingConsent: false,
  archived: false,
  createdAt: 1_790_000_000_000,
  updatedAt: 1_790_000_000_000,
};

export const customerHitsFixture: CustomerHit[] = [
  {
    id: customerFixture.id,
    code: customerFixture.code,
    name: customerFixture.name,
    customerType: "reseller",
    phone: customerFixture.phone,
    wilaya: "16",
    balance: 300_000,
    creditLimit: 500_000,
    availableCredit: 200_000,
    storeCredit: 0,
    isBlocked: false,
    isFavourite: true,
    archived: false,
  },
  {
    id: "c-nadia",
    code: "C00002",
    name: "Nadia Kaci",
    customerType: "individual",
    phone: "0661234567",
    wilaya: "16",
    balance: 0,
    creditLimit: null,
    availableCredit: null,
    storeCredit: 0,
    isBlocked: false,
    isFavourite: false,
    archived: false,
  },
];

function entry(overrides: Partial<LedgerEntry>): LedgerEntry {
  return {
    id: "e-0",
    partyId: customerFixture.id,
    kind: "sale_credit",
    amount: 0,
    balanceAfter: 0,
    method: null,
    reference: null,
    chequeBank: null,
    chequeDate: null,
    stampDuty: 0,
    stampCharged: 0,
    receiptNumber: null,
    refType: null,
    refId: null,
    refNumber: null,
    cashSessionId: "s-1",
    businessDate: "2026-09-28",
    note: null,
    createdAt: 1_790_593_200_000,
    userId: "u-yacine",
    terminalId: "t-1",
    open: 0,
    ...overrides,
  };
}

/** A ticket on credit (5 000 DA), then a versement of 2 000 DA in cash with 20 DA of duty. */
export const ledgerEntriesFixture: LedgerEntry[] = [
  entry({
    id: "e-sale",
    kind: "sale_credit",
    amount: 500_000,
    balanceAfter: 500_000,
    refType: "document",
    refId: "d-12",
    refNumber: "TK2026-C1-000012",
    businessDate: "2026-09-12",
    open: 300_000,
  }),
  entry({
    id: "e-versement",
    kind: "payment",
    amount: -200_000,
    balanceAfter: 300_000,
    method: "cash",
    stampDuty: 2_000,
    stampCharged: 2_000,
    receiptNumber: "RV2026-000001",
  }),
];

/** The notebook as `credit_entries` sends it: newest first, one page. */
export const ledgerPageFixture: EntryPage = {
  entries: [...ledgerEntriesFixture].reverse(),
  truncated: false,
  hasOpening: false,
};

export const collectQuoteFixture: CollectQuote = {
  amount: 200_000,
  stampDuty: 2_000,
  stampCharged: 2_000,
  stampMode: "charge_customer",
  toReceive: 202_000,
  balanceBefore: 300_000,
  balanceAfter: 100_000,
};

export const statementFixture: Statement = {
  partyId: customerFixture.id,
  from: "2026-09-01",
  to: "2026-09-30",
  opening: 0,
  entries: ledgerEntriesFixture,
  debits: 500_000,
  credits: 200_000,
  closing: 300_000,
};

export const supplierFixture: Supplier = {
  id: "s-cevital",
  code: "F00001",
  name: "Cevital",
  phone: "034 12 34 56",
  phone2: null,
  email: null,
  address: "Nouveau quai, Béjaïa",
  wilaya: "06",
  nif: "000006001112223",
  nis: null,
  rc: null,
  ai: null,
  nin: null,
  rib: null,
  paymentTermsDays: 30,
  balance: 500_000,
  notes: null,
  archived: false,
  createdAt: 1_790_000_000_000,
  updatedAt: 1_790_000_000_000,
};

export const purchaseSummariesFixture: PurchaseSummary[] = [
  {
    id: "d-bc1",
    docType: "purchase_order",
    number: "BC2026-C1-000001",
    status: "validated",
    orderStatus: "partial",
    businessDate: "2026-09-20",
    supplierId: supplierFixture.id,
    supplierName: supplierFixture.name,
    supplierRef: null,
    parentId: null,
    totalHt: 1_200_000,
    taxTotal: 228_000,
    totalTtc: 1_428_000,
    validatedAt: 1_790_400_000_000,
  },
  {
    id: "d-rc1",
    docType: "goods_receipt",
    number: "RC2026-C1-000001",
    status: "validated",
    orderStatus: null,
    businessDate: "2026-09-22",
    supplierId: supplierFixture.id,
    supplierName: supplierFixture.name,
    supplierRef: "BL-778",
    parentId: "d-bc1",
    totalHt: 600_000,
    taxTotal: 114_000,
    totalTtc: 714_000,
    validatedAt: 1_790_570_000_000,
  },
];

/** Order BC2026-C1-000001 still waits for 24 of its 48 UHT milks at 90,00 DA HT each. */
export const remainingFixture: RemainingLine[] = [
  {
    line: {
      id: "l-bc1-1",
      lineNo: 1,
      productId: "p-lait",
      description: "Lait UHT 1 L",
      descriptionAlt: null,
      qty: 48_000,
      unitCode: "pc",
      unitPrice: 9_000,
      priceOverridden: false,
      discountBp: 0,
      discountAmount: 0,
      taxRateBp: 900,
      totalHt: 432_000,
      taxAmount: 38_880,
      totalTtc: 470_880,
      costPrice: 0,
      lotId: null,
      serialNo: null,
      parentLineId: null,
      originLineId: null,
      restockLocationId: null,
      reasonCode: null,
      note: null,
    },
    received: 24_000,
    remaining: 24_000,
  },
];

/** The UHT milk is received by lot, with its expiry date (Task 19's product extras). */
export const lotTrackedExtrasFixture: ProductExtras = {
  productId: "p-lait",
  brand: null,
  supplierRef: null,
  altRefs: null,
  tags: null,
  attributes: {},
  minPrice: null,
  trackLots: true,
  trackSerials: false,
  preferredSupplierId: null,
  warrantyMonths: null,
};

/** Cevital's account: an invoice of 7 140 DA, then 2 140 DA paid by cheque. */
export const supplierEntriesFixture: LedgerEntry[] = [
  entry({
    id: "e-ff1",
    partyId: "s-cevital",
    kind: "purchase",
    amount: 714_000,
    balanceAfter: 714_000,
    refType: "document",
    refId: "d-ff1",
    refNumber: "FF2026-C1-000001",
    businessDate: "2026-09-22",
    open: 500_000,
  }),
  entry({
    id: "e-pay1",
    partyId: "s-cevital",
    kind: "payment",
    amount: -214_000,
    balanceAfter: 500_000,
    method: "cheque",
    reference: "0012345",
    cashSessionId: null,
    businessDate: "2026-09-25",
  }),
];

/** Cevital's account as `suppliers_entries` sends it: newest first, one page. */
export const supplierEntriesPageFixture: EntryPage = {
  entries: [...supplierEntriesFixture].reverse(),
  truncated: false,
  hasOpening: false,
};

/** An open full count of the shop floor, two lines, none counted yet. */
export const countFixture: StockCount = {
  id: "c-1",
  locationId: "00000000-0000-7000-8000-000000000301",
  scope: "full",
  categoryId: null,
  blind: false,
  status: "open",
  note: null,
  startedAt: 1_790_600_000_000,
  closedAt: null,
  userId: "u-storekeeper",
  lines: 2,
  counted: 0,
};

export const countLinesFixture: CountLineView[] = [
  {
    productId: "p-huile",
    reference: "P000012",
    barcode: "6130000000015",
    nameAr: "زيت 5 ل",
    nameFr: "Huile 5 L",
    nameEn: "Oil 5 L",
    unitCode: "pc",
    unitDecimals: 0,
    expectedQty: 10_000,
    countedQty: null,
    variance: null,
    unitCost: null,
    varianceValue: null,
  },
  {
    productId: "p-lait",
    reference: "P000013",
    barcode: "6130000000022",
    nameAr: "حليب 1 ل",
    nameFr: "Lait UHT 1 L",
    nameEn: "UHT milk 1 L",
    unitCode: "pc",
    unitDecimals: 0,
    expectedQty: 24_000,
    countedQty: null,
    variance: null,
    unitCost: null,
    varianceValue: null,
  },
];

/** The seeded shop floor (the default location), a warehouse and the damaged goods. */
export const locationsFixture: StockLocation[] = [
  {
    id: "00000000-0000-7000-8000-000000000301",
    code: "SHOP",
    nameAr: "المحل",
    nameFr: "Magasin",
    nameEn: "Shop",
    kind: "storage",
    isDefault: true,
    archived: false,
  },
  {
    id: "loc-depot",
    code: "DEPOT",
    nameAr: "المستودع",
    nameFr: "Dépôt",
    nameEn: "Warehouse",
    kind: "storage",
    isDefault: false,
    archived: false,
  },
  {
    id: "00000000-0000-7000-8000-000000000302",
    code: "DAMAGED",
    nameAr: "سلع تالفة",
    nameFr: "Marchandise abîmée",
    nameEn: "Damaged goods",
    kind: "damaged",
    isDefault: false,
    archived: false,
  },
];

/** A yoghurt lot expired three days ago and a milk lot expiring in five, both at the shop. */
export const expiryFixture: ExpiryRow[] = [
  {
    productId: "p-yaourt",
    nameAr: "ياغورت طبيعي",
    nameFr: "Yaourt nature",
    nameEn: "Plain yoghurt",
    lotId: "lot-l0901",
    lotNumber: "L0901",
    expiryDate: "2026-09-27",
    locationId: "00000000-0000-7000-8000-000000000301",
    qty: 6_000,
    daysLeft: -3,
    status: "expired",
  },
  {
    productId: "p-lait",
    nameAr: "حليب 1 ل",
    nameFr: "Lait UHT 1 L",
    nameEn: "UHT milk 1 L",
    lotId: "lot-l2609",
    lotNumber: "L2609",
    expiryDate: "2026-10-05",
    locationId: "00000000-0000-7000-8000-000000000301",
    qty: 24_000,
    daysLeft: 5,
    status: "soon",
  },
];

/** The milk under its minimum at the shop, the oil well stocked; costs as a storekeeper sees
 * them (zeroed without `reports.profit`). */
export const levelsFixture: LevelRow[] = [
  {
    productId: "p-lait",
    reference: "P000013",
    nameAr: "حليب 1 ل",
    nameFr: "Lait UHT 1 L",
    nameEn: "UHT milk 1 L",
    unitCode: "pc",
    locationId: "00000000-0000-7000-8000-000000000301",
    qty: 2_000,
    minQty: 6_000,
    costPrice: 0,
    costValue: 0,
    sellPrice: 15_000,
    retailValue: 30_000,
    archived: false,
  },
  {
    productId: "p-huile",
    reference: "P000012",
    nameAr: "زيت 5 ل",
    nameFr: "Huile 5 L",
    nameEn: "Oil 5 L",
    unitCode: "pc",
    locationId: "00000000-0000-7000-8000-000000000301",
    qty: 10_000,
    minQty: 0,
    costPrice: 0,
    costValue: 0,
    sellPrice: 95_000,
    retailValue: 950_000,
    archived: false,
  },
];

/** A named 5 % discount the cashier applies by hand (PRM-003). */
export const promotionFixture: Promotion = {
  id: "promo-1",
  nameAr: "تخفيض الزبون الوفي",
  nameFr: "Remise fidèle",
  nameEn: "Loyal customer discount",
  kind: "percent",
  campaign: "promotion",
  isLegalSoldes: false,
  triggerMode: "manual",
  scope: { kind: "all" },
  value: 500,
  buyQty: null,
  getQty: null,
  minQty: 0,
  priority: 0,
  startsOn: null,
  endsOn: null,
  active: true,
  archived: false,
  outsideSoldesWindow: false,
  products: 0,
};

/** A taxi paid 800 DA from the drawer. */
export const expenseFixture: Expense = {
  id: "e-1",
  categoryId: "00000000-0000-7000-8000-000000000505",
  categoryCode: "transport",
  nameAr: "النقل",
  nameFr: "Transport",
  nameEn: "Transport",
  amount: 80_000,
  method: "cash",
  fromTill: true,
  cashSessionId: "s-1",
  reference: null,
  supplierId: null,
  note: null,
  reversalOf: null,
  reversed: false,
  businessDate: "2026-09-29",
  createdAt: 1_790_650_000_000,
  userId: "u-cashier",
};

/** "Ventes par produit" in French, as `reports_run` answers it: a payment method tag, money,
 * quantities, a share, a code, and the totals row. */
export const reportViewFixture: ReportView = {
  report: {
    kind: "sales_by_product",
    from: "2026-09-01",
    to: "2026-09-30",
    table: {
      columns: [
        { key: "date", kind: "date" },
        { key: "name", kind: "text" },
        { key: "method", kind: "tag", tag: "payment_method" },
        { key: "qty", kind: "qty" },
        { key: "netTtc", kind: "money" },
        { key: "share", kind: "percent" },
        { key: "reference", kind: "code" },
      ],
      rows: [
        ["2026-09-28", "Huile 5 L", "cash", 1_500, 119_000, 1_615, "0012"],
        ["2026-09-28", "Sucre 1 kg", "card_cib", 3_000, -39_000, 8_385, "0100"],
      ],
      totals: [null, null, null, null, 80_000, null, null],
    },
    chart: { kind: "h_bar", label: "name", value: "netTtc" },
    truncated: false,
  },
  title: "Ventes par produit",
  subtitle: "Du 01/09/2026 au 30/09/2026",
  labels: ["Date", "Nom", "Mode de paiement", "Quantité", "Net TTC", "Part", "Référence"],
  tagLabels: { "payment_method.card_cib": "Carte CIB", "payment_method.cash": "Espèces" },
};

/** Today (2026-09-29): three tickets for 2 380 DA, as the owner sees it (every figure present). */
export const dashboardFixture: Dashboard = {
  businessDate: "2026-09-29",
  netTtc: 238_000,
  tickets: 3,
  averageTicket: 79_333,
  returnsTtc: 0,
  yesterdayNetTtc: 131_000,
  monthNetTtc: 3_764_000,
  margin: 61_500,
  receivables: 300_000,
  payables: 450_000,
  lowStock: 2,
  expiring: 1,
  openSessions: 1,
  lastDays: Array.from({ length: 14 }, (_, i) => ({
    day: `2026-09-${String(16 + i).padStart(2, "0")}`,
    netTtc: i === 13 ? 238_000 : 120_000 + i * 1_000,
  })),
  topProducts: [
    { productId: "p-oil", name: "Huile 5 L", qty: 1_000, netTtc: 119_000 },
    { productId: "p-sugar", name: "Sucre 1 kg", qty: 5_000, netTtc: 65_000 },
    { productId: "p-milk", name: "Lait UHT 1 L", qty: 6_000, netTtc: 54_000 },
  ],
  byMethod: [
    { method: "cash", net: 188_000 },
    { method: "card_cib", net: 50_000 },
  ],
};

/** `articles.csv`: a semicolon CSV of a header and three products. */
export const importFileFixture: ImportFile = {
  token: "t-articles",
  fileName: "articles.csv",
  format: "csv",
  encoding: "UTF-8",
  delimiter: ";",
  sheets: [{ name: "", rows: 4 }],
};

/** The product fields with their French labels (the two extra price levels last). */
const productField = (
  key: string,
  label: string,
  kind: ImportPreview["fields"][number]["kind"],
  identifies = false,
): ImportPreview["fields"][number] => ({ key, kind, required: false, identifies, choice: null, label });

/** The preview of `articles.csv` for a product import; the third price has an ambiguous
 * separator (`1,234`). */
export const importPreviewFixture: ImportPreview = {
  headerRow: 0,
  headers: ["Référence", "Désignation", "Prix de vente"],
  sample: [
    ["A1", "Huile 5 L", "1190,00"],
    ["A2", "Sucre 1 kg", "130,00"],
    ["A3", "Lait UHT 1 L", "1,234"],
  ],
  fields: [
    productField("reference", "Référence", "code", true),
    productField("barcode", "Code-barres", "list", true),
    productField("nameFr", "Nom (français)", "text"),
    productField("nameAr", "Nom (arabe)", "text"),
    productField("nameEn", "Nom (anglais)", "text"),
    productField("category", "Catégorie", "text"),
    productField("unit", "Unité", "code"),
    productField("taxRate", "Taux de TVA", "percent"),
    productField("sellPrice", "Prix de vente", "money"),
    productField("costPrice", "Prix de revient", "money"),
    productField("minStock", "Stock minimum", "qty"),
    productField("trackStock", "Suivi du stock", "bool"),
    productField("brand", "Marque", "text"),
    productField("supplierRef", "Référence fournisseur", "code"),
    productField("price.semi_wholesale", "Demi-gros", "money"),
    productField("price.wholesale", "Gros", "money"),
  ],
  suggested: ["reference", "nameFr", "sellPrice"],
};

/** The expense categories seeded by migration 0003, each in its cash group. */
export const expenseCategoriesFixture: ExpenseCategoryInfo[] = [
  ["501", "rent", "الإيجار", "Loyer", "Rent"],
  ["502", "electricity", "الكهرباء والغاز", "Électricité et gaz", "Electricity and gas"],
  ["503", "water", "الماء", "Eau", "Water"],
  ["504", "wages", "الأجور", "Salaires", "Wages"],
  ["505", "transport", "النقل", "Transport", "Transport"],
  ["506", "supplies", "اللوازم", "Fournitures", "Supplies"],
  ["507", "maintenance", "الصيانة", "Entretien", "Maintenance"],
  ["508", "other", "أخرى", "Autres", "Other"],
].map(([n, code, nameAr, nameFr, nameEn], i) => ({
  id: `00000000-0000-7000-8000-000000000${n}`,
  code: code!,
  nameAr: nameAr!,
  nameFr: nameFr!,
  nameEn: nameEn!,
  cashGroup: code as ExpenseCategoryInfo["cashGroup"],
  sortOrder: i + 1,
  archived: false,
}));

/** This morning's import of `articles.csv`: two products created, one row refused; the latest
 * import of the day, so it can be undone. */
export const importJobFixture: ImportJob = {
  id: "j-1",
  kind: "products",
  fileName: "articles.csv",
  policy: "create_only",
  status: "done",
  total: 3,
  created: 2,
  updated: 0,
  skipped: 0,
  failed: 1,
  problems: [{ row: 4, field: "sellPrice", value: "1,234", code: "ambiguous_separator" }],
  createdAt: 1_790_650_000_000,
  finishedAt: 1_790_650_002_000,
  userId: "u-manager",
  undoable: true,
};
