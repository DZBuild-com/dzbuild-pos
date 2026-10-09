import type {
  ArchivePage,
  CreditableLine,
  CreditQuote,
  DocState,
  Document,
  DraftPreview,
  MentionCheck,
} from "@/lib/ipc/types";
import { documentFixture, previewFixture } from "./fixtures-m1";

export const mentionsCompleteFixture: MentionCheck = {
  sellerMissing: [],
  sellerWarnings: [],
  buyerKind: "business",
  buyerMissing: [],
  buyerWarnings: [],
};

export const mentionsMissingFixture: MentionCheck = {
  sellerMissing: ["nis"],
  sellerWarnings: ["capital"],
  buyerKind: "business",
  buyerMissing: ["rc", "nif"],
  buyerWarnings: [],
};

export const docStateFixture: DocState = {
  state: "unpaid",
  remaining: 1_800_000,
  dueDate: "2026-10-28",
  overdue: false,
};

export const archivePageFixture: ArchivePage = {
  rows: [
    {
      id: "inv-1",
      docType: "invoice",
      number: "FA2026-000045",
      status: "validated",
      businessDate: "2026-09-28",
      sortAt: 1_790_600_000_000,
      partyName: "SARL Amine",
      totalTtc: 2_339_500,
      totalDue: 2_344_900,
      origin: null,
      parentNumber: null,
      state: docStateFixture,
      outputs: 1,
    },
    {
      id: "q-1",
      docType: "quote",
      number: "DV2026-000012",
      status: "validated",
      businessDate: "2026-09-27",
      sortAt: 1_790_500_000_000,
      partyName: "SARL Amine",
      totalTtc: 119_000,
      totalDue: 119_000,
      origin: null,
      parentNumber: null,
      state: { state: "open", validUntil: "2026-10-27" },
      outputs: 0,
    },
  ],
  next: null,
};

export const draftPreviewFixture: DraftPreview = {
  priced: { cart: previewFixture(119_000, 0, 0).cart, vatExempt: false },
  mentions: mentionsCompleteFixture,
};

export const validatedInvoiceFixture: Document = {
  ...documentFixture,
  id: "inv-1",
  docType: "invoice",
  number: "FA2026-000045",
  series: "FA",
  status: "validated",
  businessDate: "2026-09-28",
  customerId: "c-amine",
  cashSessionId: null,
  taxRegime: "reel",
  priceMode: "ht",
  totalTtc: 2_339_500,
  stampDuty: 5_400,
  totalDue: 2_344_900,
  paidTotal: 544_900,
  creditAmount: 1_800_000,
  validUntil: null,
  dueDate: "2026-10-28",
  secondLang: null,
  creditKind: null,
  origin: null,
  parties: [],
};

export const draftQuoteFixture: Document = {
  ...validatedInvoiceFixture,
  id: "d1",
  docType: "quote",
  number: null,
  series: "DV",
  status: "draft",
  totalTtc: 119_000,
  stampDuty: 0,
  totalDue: 119_000,
  paidTotal: 0,
  creditAmount: 0,
  dueDate: null,
  validUntil: "2026-10-28",
};

export const creditableFixture: CreditableLine[] = [
  {
    line: {
      ...documentFixture.lines[0]!,
      id: "l-cement",
      description: "Ciment 25 kg",
      qty: 10_000,
      unitPrice: 100_000,
    },
    credited: 0,
    remaining: 10_000,
    remainingTtc: 1_190_000,
  },
];

export const creditQuoteFixture: CreditQuote = {
  totalTtc: -357_000,
  taxTotal: -57_000,
  refund: 357_000,
  invoiceRemaining: 1_982_500,
};
