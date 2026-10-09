import type {
  DisplayFrame,
  HardwareConfig,
  LabelBatch,
  LabelSheet,
  LabelTemplatesView,
  ScaleItem,
  StaleLabel,
  WeightEvent,
} from "@/lib/ipc/types";

const fields = {
  name: true,
  price: true,
  unit: true,
  barcode: true,
  reference: false,
  lot: false,
  expiry: false,
  promo: true,
  shop: false,
  weight: false,
  packedOn: false,
};

/** The two built-in roll templates the tests use, three size presets and two A4 sheets. */
export const labelTemplatesFixture: LabelTemplatesView = {
  templates: [
    {
      id: "builtin.price_40x25",
      name: "",
      widthDmm: 400,
      heightDmm: 250,
      gapDmm: 20,
      columns: 1,
      columnGapDmm: 0,
      lang: "ar_fr",
      fields,
      symbology: "auto",
      nameLines: 1,
    },
    {
      id: "builtin.weighed_58x40",
      name: "",
      widthDmm: 580,
      heightDmm: 400,
      gapDmm: 20,
      columns: 1,
      columnGapDmm: 0,
      lang: "ar_fr",
      fields: { ...fields, promo: false, weight: true, packedOn: true, expiry: true, shop: true },
      symbology: "auto",
      nameLines: 1,
    },
  ],
  categoryTemplates: {},
  sizePresets: [
    { widthMm: 40, heightMm: 25 },
    { widthMm: 50, heightMm: 30 },
    { widthMm: 58, heightMm: 40 },
  ],
  sheetFormats: [
    {
      id: "a4_24_70x37",
      columns: 3,
      rows: 8,
      labelWDmm: 700,
      labelHDmm: 370,
      marginTopDmm: 5,
      marginStartDmm: 0,
      gapXDmm: 0,
      gapYDmm: 0,
    },
    {
      id: "a4_65_38x21",
      columns: 5,
      rows: 13,
      labelWDmm: 381,
      labelHDmm: 212,
      marginTopDmm: 107,
      marginStartDmm: 47,
      gapXDmm: 25,
      gapYDmm: 0,
    },
  ],
};

/** Two French-only labels of Candia milk, refused until the Arabic guard is answered. */
export const labelBatchFixture: LabelBatch = {
  groups: [
    {
      template: labelTemplatesFixture.templates[0]!,
      labels: [
        {
          productId: "p-milk",
          copies: 2,
          lang: "fr",
          name: "Lait Candia 1 L",
          nameSecond: null,
          reference: null,
          price: 15_000,
          oldPrice: null,
          unitCode: "pc",
          unitPrice: null,
          weight: null,
          lotNumber: null,
          expiry: null,
          packedOn: null,
          shopName: null,
          barcode: { symbology: "ean13", data: "6130123456782" },
        },
      ],
    },
  ],
  warnings: [{ productId: "p-milk", name: "Lait Candia 1 L", code: "arabic_required" }],
  total: 2,
};

/** The two milk labels of {@link labelBatchFixture} on a 3 × 8 planche from position `start`, as
 * Rust lays them out (French only: the Arabic guard was answered). */
export function labelSheetFixture(start: number): LabelSheet {
  const format = labelTemplatesFixture.sheetFormats[0]!;
  const cells = [start - 1, start].map((slot) => {
    const pos = slot % (format.columns * format.rows);
    const [row, column] = [Math.floor(pos / format.columns), pos % format.columns];
    return {
      page: Math.floor(slot / (format.columns * format.rows)),
      row,
      column,
      xDmm: format.marginStartDmm + column * format.labelWDmm,
      yDmm: format.marginTopDmm + row * format.labelHDmm,
      label: 0,
    };
  });
  return {
    format,
    pages: cells.at(-1)!.page + 1,
    cells,
    labels: [
      {
        rtl: false,
        lang: "fr",
        shop: null,
        name: "Lait Candia 1 L",
        nameSecond: null,
        price: "150,00 DA",
        oldPrice: null,
        weight: null,
        details: [],
        barcode: { symbology: "ean13", data: "6130123456782", rows: ["101"], hri: "6130123456782" },
      },
    ],
  };
}

/** Candia milk printed at 150 DA, now 160 DA. */
export const staleLabelsFixture: StaleLabel[] = [
  {
    productId: "p-milk",
    nameAr: "حليب كانديا",
    nameFr: "Lait Candia 1 L",
    nameEn: "",
    printedAt: 1_790_593_200_000,
    printedPrice: 15_000,
    printedOldPrice: null,
    price: 16_000,
    oldPrice: null,
    reasons: ["price"],
  },
];

/** Tomatoes at 120 DA/kg on scale key 3, 120 g tare, 3 days' shelf life. */
export const scaleItemsFixture: ScaleItem[] = [
  {
    productId: "p-tomato",
    plu: 12,
    reference: null,
    nameAr: "طماطم",
    nameFr: "Tomates",
    nameEn: "",
    unitCode: "kg",
    isWeighed: true,
    price: 12_000,
    hotkey: 3,
    tareG: 120,
    shelfLifeDays: 3,
    barcodePrefix: "2100012",
  },
];

/** `hardware.json` v2 of a PC with nothing plugged in. */
export const hardwareV2Fixture: HardwareConfig = {
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
  scanner: { layout: "us", prefix: "", suffix: "enter_or_tab", serial: null },
  labelPrinters: [],
  labelPrinterId: null,
  scale: null,
  display: { vfd: null, screen: { enabled: false, monitor: null, showLogo: true, thanksSeconds: 5 } },
  drawer: { kind: "printer" },
  driverNames: {},
};

export const weightFixture: WeightEvent = { gross: 1_250, net: 1_250, tare: 0, stable: true, state: "ok" };

export const frameFixture: DisplayFrame = {
  mode: "sale",
  lang: "fr",
  items: 3_000,
  total: 45_000,
  saved: 0,
  paid: null,
  change: null,
  weight: null,
  line: {
    name: "Lait Candia 1 L",
    latinName: "Lait Candia 1 L",
    qty: 3_000,
    unitCode: "pc",
    unitPrice: 15_000,
    total: 45_000,
  },
};
